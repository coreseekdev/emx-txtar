//! Archive data structures

use std::borrow::Cow;
use std::path::Path;

// Txtar format constants
pub const MARKER_PREFIX: &str = "-- ";
pub const MARKER_SUFFIX: &str = " --";
pub const MARKER_PREFIX_LEN: usize = 3;  // len("-- ")
pub const MARKER_SUFFIX_LEN: usize = 3;  // len(" --")
pub const BASE64_SUFFIX: &str = "[.base64]";
pub const BASE64_SUFFIX_LEN: usize = 9; // len("[.base64]") = 1 + 1 + 6 + 1

/// Configuration for encoding detection
#[derive(Debug, Clone)]
pub struct EncodingConfig {
    /// Whether to check file content for conflicting marker patterns
    pub check_content_markers: bool,
    /// Whether to validate UTF-8 encoding (if false, treats all non-UTF8 as binary)
    pub validate_utf8: bool,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            check_content_markers: true,
            validate_utf8: true,
        }
    }
}

/// Result of encoding detection
#[derive(Debug, Clone, PartialEq)]
pub enum EncodingDetection {
    /// Valid text with specific encoding
    Text { encoding: TextEncoding },
    /// Binary data that needs base64 encoding
    Binary { reason: BinaryReason },
}

/// Text encoding type (extensible for i18n)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEncoding {
    /// UTF-8 text
    Utf8,
    // Future: Utf16Le, Utf16Be, GBK, ShiftJIS, etc.
}

/// Reason why data is considered binary
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryReason {
    /// Content contains txtar marker pattern (-- filename --)
    /// This is the primary cause for binary encoding
    ContentConflict,
    /// Invalid UTF-8 encoding (actual binary data)
    InvalidUtf8,
    /// Explicitly marked as binary by user
    Explicit,
}

/// Represents a single file in an archive
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Name of the file (may include subdirectories)
    pub name: String,
    /// Contents of the file
    pub data: Vec<u8>,
    /// Whether this file is binary encoded
    pub is_binary: bool,
    /// Reason for binary encoding (if applicable)
    pub binary_reason: Option<BinaryReason>,
    /// Snippet reference if this is a code snippet
    pub snippet_ref: Option<SnippetRef>,
    /// Edit reference if this file contains edit instructions
    pub edit_ref: Option<EditRef>,
}

impl File {
    /// Create a new file with the given name and data
    /// Uses default config to auto-detect encoding
    pub fn new(name: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self::with_config(name, data, &EncodingConfig::default())
    }

    /// Create a file with explicit binary encoding flag
    pub fn with_encoding(name: impl Into<String>, data: impl Into<Vec<u8>>, is_binary: bool) -> Self {
        Self {
            name: name.into(),
            data: data.into(),
            is_binary,
            binary_reason: if is_binary { Some(BinaryReason::Explicit) } else { None },
            snippet_ref: None,
            edit_ref: None,
        }
    }

    /// Create a file with custom encoding detection config
    pub fn with_config(name: impl Into<String>, data: impl Into<Vec<u8>>, config: &EncodingConfig) -> Self {
        let name = name.into();
        let data = data.into();

        let detection = Self::detect_encoding(&name, &data, config);

        match detection {
            EncodingDetection::Text { .. } => Self {
                name,
                data,
                is_binary: false,
                binary_reason: None,
                snippet_ref: None,
                edit_ref: None,
            },
            EncodingDetection::Binary { reason } => Self {
                name,
                data,
                is_binary: true,
                binary_reason: Some(reason),
                snippet_ref: None,
                edit_ref: None,
            },
        }
    }

    /// Detect the encoding of file data
    pub fn detect_encoding(_name: &str, data: &[u8], config: &EncodingConfig) -> EncodingDetection {
        // Check content for conflicting marker patterns (if enabled)
        // This is the REAL issue: content containing "-- filename --" patterns
        // will be parsed as new file entries in the archive
        if config.check_content_markers {
            if let Ok(text) = std::str::from_utf8(data) {
                if Self::contains_marker_pattern(text) {
                    return EncodingDetection::Binary {
                        reason: BinaryReason::ContentConflict,
                    };
                }
            }
        }

        // Check UTF-8 encoding (if enabled)
        if config.validate_utf8 {
            if std::str::from_utf8(data).is_err() {
                return EncodingDetection::Binary {
                    reason: BinaryReason::InvalidUtf8,
                };
            }
        }

        // Valid text (currently only UTF-8)
        EncodingDetection::Text {
            encoding: TextEncoding::Utf8,
        }
    }

    /// Check if text contains txtar marker pattern `-- xxx --`
    fn contains_marker_pattern(text: &str) -> bool {
        // Look for lines that match the marker pattern
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(MARKER_PREFIX) && trimmed.ends_with(MARKER_SUFFIX) {
                // Extract what's between the markers
                let content = &trimmed[MARKER_PREFIX_LEN..trimmed.len() - MARKER_SUFFIX_LEN];
                // If it's not empty and looks like a filename (not just spaces)
                if !content.trim().is_empty() {
                    return true;
                }
            }
        }
        false
    }

    /// Get the formatted name for the archive header
    /// If binary encoding is needed, appends `[.base64]` suffix
    pub fn archive_name(&self) -> String {
        if self.is_binary {
            format!("{}{}", self.name, BASE64_SUFFIX)
        } else {
            self.name.clone()
        }
    }

    /// Parse an archive name, extracting the real name and binary flag
    pub fn parse_archive_name(archive_name: &str) -> (String, bool) {
        if archive_name.ends_with(BASE64_SUFFIX) {
            let name = &archive_name[..archive_name.len() - BASE64_SUFFIX_LEN];
            (name.to_string(), true)
        } else {
            (archive_name.to_string(), false)
        }
    }
}

/// A command reference stored in the archive comment
/// Format: [command: cmd](#href)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The command name/type (e.g., "rg", "sed")
    pub name: String,
    /// The href identifier (without the # prefix)
    pub href: String,
}

/// A snippet reference for a file
/// Format: [.snippet:N] or .#href:line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetRef {
    /// Optional command reference (if .#href:line format)
    pub command_href: Option<String>,
    /// Line number in the original source
    pub line: usize,
}

/// Operation type for an edit block
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOperation {
    /// Replace content (both SEARCH and REPLACE present)
    Replace,
    /// Delete content (only SEARCH present)
    Delete,
    /// Insert content (only REPLACE present)
    Insert,
}

/// A single edit block (SEARCH/REPLACE pair)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditBlock {
    /// Original content (SEARCH block), lines trimmed for trailing whitespace
    pub search: Vec<String>,
    /// New content (REPLACE block), lines trimmed for trailing whitespace
    pub replacement: Vec<String>,
    /// Operation type
    pub operation: EditOperation,
}

/// Edit reference for applying changes to files
/// Format: [.edit] or [.edit#href:line]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRef {
    /// Optional command reference (metadata about where this edit came from)
    pub command_href: Option<String>,
    /// Optional starting line number (for information only, not used for application)
    pub start_line: Option<usize>,
    /// Edit blocks to apply (typically one, but multiple allowed)
    pub edits: Vec<EditBlock>,
}

/// Error type for snippet reference parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetParseError {
    /// Input doesn't match any known format
    InvalidFormat,
    /// Missing closing bracket ']'
    MissingClosingBracket,
    /// Missing colon ':' in href:line or snippet:N
    MissingColon,
    /// Line number is not a valid number
    InvalidLineNumber { input: String },
}

impl std::fmt::Display for SnippetParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnippetParseError::InvalidFormat => {
                write!(f, "Invalid snippet format. Expected [.snippet:N], [.snippet#href:line], or [.#href:line]")
            }
            SnippetParseError::MissingClosingBracket => {
                write!(f, "Missing closing bracket ']'")
            }
            SnippetParseError::MissingColon => {
                write!(f, "Missing colon ':' in href:line or snippet:N format")
            }
            SnippetParseError::InvalidLineNumber { input } => {
                write!(f, "Invalid line number: '{}'", input)
            }
        }
    }
}

impl std::error::Error for SnippetParseError {}

/// Error type for edit block parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditParseError {
    /// Edit block is not properly terminated (missing >>>>>>> marker)
    UnterminatedBlock,

    /// Invalid or unknown marker format
    InvalidMarker { marker: String },

    /// Empty edit block (both search and replacement are empty)
    EmptyBlock,

    /// Invalid parser state (internal error)
    InvalidState { state: String },

    /// Malformed line in edit content
    MalformedLine { line_number: usize, line: String },

    /// Expected <<<<<<< SEARCH marker at the beginning
    ExpectedSearchStart,

    /// Expected ======= separator
    ExpectedSeparator,

    /// Expected >>>>>>> REPLACE or >>>>>>> DELETE marker
    ExpectedEndMarker,

    /// Missing closing bracket in tag format
    MissingClosingBracket,
}

impl std::fmt::Display for EditParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditParseError::UnterminatedBlock => {
                write!(f, "Unterminated edit block (missing >>>>>>> marker)")
            }
            EditParseError::InvalidMarker { marker } => {
                write!(f, "Invalid marker format: '{}'. Expected <<<<<<< SEARCH, =======, >>>>>>> REPLACE, or >>>>>>> DELETE", marker)
            }
            EditParseError::EmptyBlock => {
                write!(f, "Empty edit block (both search and replacement are empty)")
            }
            EditParseError::InvalidState { state } => {
                write!(f, "Invalid parser state: {}", state)
            }
            EditParseError::MalformedLine { line_number, line } => {
                write!(f, "Malformed line at {}: '{}'", line_number, line)
            }
            EditParseError::ExpectedSearchStart => {
                write!(f, "Expected <<<<<<< SEARCH marker at the beginning of edit block")
            }
            EditParseError::ExpectedSeparator => {
                write!(f, "Expected ======= separator after SEARCH block")
            }
            EditParseError::ExpectedEndMarker => {
                write!(f, "Expected >>>>>>> REPLACE or >>>>>>> DELETE marker at the end of edit block")
            }
            EditParseError::MissingClosingBracket => {
                write!(f, "Missing closing bracket ']' in tag format")
            }
        }
    }
}

impl std::error::Error for EditParseError {}

/// Error type for edit application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditApplyError {
    /// Search pattern not found in content
    SearchNotFound { search: String },

    /// Search pattern found multiple times (ambiguous)
    MultipleMatches { search: String, count: usize },

    /// Invalid line number reference
    InvalidLineNumber { line: usize, max_line: usize },

    /// Cannot apply edit to empty content
    EmptyContent,

    /// Conflicting edits (earlier edit affects later edit)
    ConflictingEdits { edit_index: usize },

    /// File content is not valid UTF-8
    InvalidUtf8,

    /// I/O error during file operations
    IoError(String),
}

impl std::fmt::Display for EditApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditApplyError::SearchNotFound { search } => {
                write!(f, "Search pattern not found: '{}'", search)
            }
            EditApplyError::MultipleMatches { search, count } => {
                write!(f, "Search pattern found {} times (ambiguous): '{}'", count, search)
            }
            EditApplyError::InvalidLineNumber { line, max_line } => {
                write!(f, "Invalid line number: {} (file has {} lines)", line, max_line)
            }
            EditApplyError::EmptyContent => {
                write!(f, "Cannot apply edit to empty content")
            }
            EditApplyError::ConflictingEdits { edit_index } => {
                write!(f, "Conflicting edit at index {}: earlier edit changed line numbers", edit_index)
            }
            EditApplyError::InvalidUtf8 => {
                write!(f, "File content is not valid UTF-8")
            }
            EditApplyError::IoError(msg) => {
                write!(f, "I/O error: {}", msg)
            }
        }
    }
}

impl std::error::Error for EditApplyError {}

impl From<std::io::Error> for EditApplyError {
    fn from(err: std::io::Error) -> Self {
        EditApplyError::IoError(err.to_string())
    }
}

impl SnippetRef {
    /// Parse a snippet reference from format: [.snippet:N], [.snippet#href:line], or [.#href:line]
    /// Note: [.#href:line] is shorthand for [.snippet#href:line]
    ///
    /// Returns Ok(SnippetRef) if successful, Err(SnippetParseError) if format is invalid
    pub fn parse(input: &str) -> Result<Self, SnippetParseError> {
        let input = input.trim();

        // Determine the format and extract inner content with href indicator
        let (inner, has_href_marker) = if let Some(rest) = input.strip_prefix("[.#") {
            // [.#href:line] format - has href marker
            let inner = rest.strip_suffix(']')
                .ok_or(SnippetParseError::MissingClosingBracket)?;
            (inner, true)
        } else if let Some(rest) = input.strip_prefix("[.snippet#") {
            // [.snippet#href:line] format - has href marker
            let inner = rest.strip_suffix(']')
                .ok_or(SnippetParseError::MissingClosingBracket)?;
            (inner, true)
        } else if let Some(rest) = input.strip_prefix("[.snippet:") {
            // [.snippet:N] format - no href marker
            let inner = rest.strip_suffix(']')
                .ok_or(SnippetParseError::MissingClosingBracket)?;
            (inner, false)
        } else {
            return Err(SnippetParseError::InvalidFormat);
        };

        // Parse based on whether we have an href marker
        if has_href_marker {
            // Format: href:line
            let colon_pos = inner.find(':')
                .ok_or(SnippetParseError::MissingColon)?;
            let href = inner[..colon_pos].to_string();
            let line_str = &inner[colon_pos + 1..];
            let line = line_str.trim().parse::<usize>()
                .map_err(|_| SnippetParseError::InvalidLineNumber { input: line_str.to_string() })?;
            Ok(SnippetRef { command_href: Some(href), line })
        } else {
            // Format: just line number
            let line = inner.trim().parse::<usize>()
                .map_err(|_| SnippetParseError::InvalidLineNumber { input: inner.to_string() })?;
            Ok(SnippetRef { command_href: None, line })
        }
    }

    /// Legacy method for backward compatibility - returns None on parse failure
    /// Use `parse()` instead for better error reporting
    #[deprecated(note = "Use parse() for better error reporting")]
    pub fn parse_legacy(input: &str) -> Option<Self> {
        Self::parse(input).ok()
    }
}

impl EditRef {
    /// Parse edit blocks from file content.
    ///
    /// Expected format (unified diff style):
    /// ```text
    /// <<<<<< SEARCH
    /// original line 1
    /// original line 2
    /// =======
    /// new line 1
    /// new line 2
    /// >>>>>> REPLACE
    /// ```
    ///
    /// Supported operations:
    /// - **Replace**: Both SEARCH and REPLACE blocks present
    /// - **Delete**: Only SEARCH block (ends with `>>>>>>> DELETE`)
    /// - **Insert**: Empty SEARCH with REPLACE content (inferred)
    ///
    /// # Returns
    /// - `Ok(Vec<EditBlock>)` - Successfully parsed edit blocks
    /// - `Err(EditParseError)` - Parse error with detailed message
    ///
    /// # Errors
    /// - `EditParseError::ExpectedSearchStart` - Content doesn't start with `<<<<<<< SEARCH`
    /// - `EditParseError::UnterminatedBlock` - Missing closing `>>>>>>>` marker
    /// - `EditParseError::EmptyBlock` - Both SEARCH and REPLACE are empty
    /// - `EditParseError::MalformedLine` - Invalid line format with line number
    pub fn parse_content(content: &str) -> Result<Vec<EditBlock>, EditParseError> {
        let mut parser = EditParser::new();
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed for error messages
            parser.parse_line(line, line_num)?;
        }

        parser.finish()
    }

    /// Apply all edit blocks to file content.
    ///
    /// This method applies each edit block sequentially to the content.
    /// Edits are applied in order, and each edit may affect the line numbers
    /// of subsequent edits.
    ///
    /// # Arguments
    /// * `content` - The original file content to modify
    ///
    /// # Returns
    /// - `Ok(String)` - Modified content after applying all edits
    /// - `Err(EditApplyError)` - Error during edit application
    ///
    /// # Errors
    /// - `EditApplyError::EmptyContent` - Cannot apply edits to empty content
    /// - `EditApplyError::SearchNotFound` - SEARCH pattern not found in content
    /// - `EditApplyError::MultipleMatches` - SEARCH pattern found multiple times
    ///
    /// # Example
    /// ```rust
    /// use emx_txtar::{EditRef, EditBlock, EditOperation};
    ///
    /// let content = "line 1\nline 2\nline 3";
    /// let edit_ref = EditRef {
    ///     command_href: None,
    ///     start_line: None,
    ///     edits: vec![
    ///         EditBlock {
    ///             search: vec!["line 2".to_string()],
    ///             replacement: vec!["modified line 2".to_string()],
    ///             operation: EditOperation::Replace,
    ///         },
    ///     ],
    /// };
    ///
    /// let result = edit_ref.apply(content)?;
    /// assert_eq!(result, "line 1\nmodified line 2\nline 3");
    /// # Ok::<(), emx_txtar::EditApplyError>(())
    /// ```
    pub fn apply(&self, content: &str) -> Result<String, EditApplyError> {
        if content.is_empty() && !self.edits.is_empty() {
            // Only allow edits on empty content if all edits are Insert operations
            for edit in &self.edits {
                if edit.operation != EditOperation::Insert {
                    return Err(EditApplyError::EmptyContent);
                }
            }
        }

        // Use Cow to avoid unnecessary allocations
        let mut lines: Vec<Cow<str>> = content.lines().map(Cow::Borrowed).collect();

        // Apply each edit sequentially
        for (edit_index, edit) in self.edits.iter().enumerate() {
            lines = self.apply_edit_to_lines(lines, edit, edit_index)?;
        }

        // Join at the end (only one allocation)
        Ok(lines.iter().map(|cow| cow.as_ref()).collect::<Vec<&str>>().join("\n"))
    }

    /// Apply a single edit block to a list of lines
    fn apply_edit_to_lines<'a>(
        &self,
        lines: Vec<Cow<'a, str>>,
        edit: &EditBlock,
        edit_index: usize,
    ) -> Result<Vec<Cow<'a, str>>, EditApplyError> {
        match edit.operation {
            EditOperation::Replace => {
                self.replace_lines(lines, &edit.search, &edit.replacement)
            }
            EditOperation::Delete => {
                self.delete_lines(lines, &edit.search)
            }
            EditOperation::Insert => {
                // Insert at the beginning if content is empty
                if lines.is_empty() {
                    Ok(edit.replacement.iter().map(|s| Cow::Owned(s.clone())).collect())
                } else {
                    // Insert at the beginning (line 0)
                    let mut result: Vec<Cow<'a, str>> = edit.replacement.iter()
                        .map(|s| Cow::Owned(s.clone()))
                        .collect();
                    result.extend(lines);
                    Ok(result)
                }
            }
        }
    }

    /// Replace lines matching search pattern with replacement
    fn replace_lines<'a>(
        &self,
        lines: Vec<Cow<'a, str>>,
        search: &[String],
        replacement: &[String],
    ) -> Result<Vec<Cow<'a, str>>, EditApplyError> {
        if search.is_empty() {
            // Empty search means insert at the beginning
            let mut result: Vec<Cow<'a, str>> = replacement.iter()
                .map(|s| Cow::Owned(s.clone()))
                .collect();
            result.extend(lines);
            return Ok(result);
        }

        let start = self.find_search_block(&lines, search)?;

        let mut result = Vec::with_capacity(lines.len() + replacement.len());

        // Add lines before the match (borrowed, no allocation)
        result.extend(lines[..start].iter().cloned());

        // Add replacement lines (owned, allocated once)
        result.extend(replacement.iter().map(|s| Cow::Owned(s.clone())));

        // Add lines after the match (borrowed, no allocation)
        result.extend(lines[start + search.len()..].iter().cloned());

        Ok(result)
    }

    /// Delete lines matching search pattern
    fn delete_lines<'a>(
        &self,
        lines: Vec<Cow<'a, str>>,
        search: &[String],
    ) -> Result<Vec<Cow<'a, str>>, EditApplyError> {
        let start = self.find_search_block(&lines, search)?;

        let mut result = Vec::with_capacity(lines.len());

        // Add lines before the match
        result.extend(lines[..start].iter().cloned());

        // Skip the search block

        // Add lines after the match
        result.extend(lines[start + search.len()..].iter().cloned());

        Ok(result)
    }

    /// Find the location of a search block in lines
    fn find_search_block(&self, lines: &[Cow<str>], search: &[String]) -> Result<usize, EditApplyError> {
        if search.is_empty() {
            return Err(EditApplyError::SearchNotFound {
                search: "(empty)".to_string(),
            });
        }

        // Try to find exact match
        for start in 0..=lines.len().saturating_sub(search.len()) {
            if lines.len() < start + search.len() {
                break;
            }

            let mut matches = true;
            for (i, search_line) in search.iter().enumerate() {
                if lines[start + i].as_ref() != search_line.as_str() {
                    matches = false;
                    break;
                }
            }

            if matches {
                return Ok(start);
            }
        }

        // Not found
        Err(EditApplyError::SearchNotFound {
            search: search.join("\n"),
        })
    }
}

/// Internal parser for edit blocks
struct EditParser {
    edits: Vec<EditBlock>,
    current_search: Option<Vec<String>>,
    current_replace: Option<Vec<String>>,
    state: ParseState,
}

impl EditParser {
    fn new() -> Self {
        Self {
            edits: Vec::new(),
            current_search: None,
            current_replace: None,
            state: ParseState::Start,
        }
    }

    fn parse_line(&mut self, line: &str, line_num: usize) -> Result<(), EditParseError> {
        let line = line.trim_end(); // Trim trailing whitespace only

        match self.state {
            ParseState::Start => self.handle_start(line, line_num),
            ParseState::InSearch => self.handle_search(line, line_num),
            ParseState::InReplace => self.handle_replace(line, line_num),
        }
    }

    fn handle_start(&mut self, line: &str, line_num: usize) -> Result<(), EditParseError> {
        if line.starts_with("<<<<<<< SEARCH") {
            self.current_search = Some(Vec::new());
            self.state = ParseState::InSearch;
            Ok(())
        } else if line.starts_with("<<<<<<<") {
            Err(EditParseError::MalformedLine {
                line_number: line_num,
                line: line.to_string(),
            })
        } else if !line.is_empty() {
            Err(EditParseError::ExpectedSearchStart)
        } else {
            Ok(()) // Skip empty lines before first edit block
        }
    }

    fn handle_search(&mut self, line: &str, _line_num: usize) -> Result<(), EditParseError> {
        if line.starts_with("=======") {
            self.state = ParseState::InReplace;
            Ok(())
        } else if line.starts_with(">>>>>>> DELETE") {
            // Delete operation (no replacement)
            let search = self.current_search.take()
                .unwrap_or_default();

            self.edits.push(EditBlock {
                search,
                replacement: Vec::new(),
                operation: EditOperation::Delete,
            });

            self.state = ParseState::Start;
            Ok(())
        } else {
            // Add line to search block (including empty lines — they are
            // significant content for matching blank lines in the target file).
            if let Some(ref mut search) = self.current_search {
                search.push(line.to_string());
                Ok(())
            } else {
                Err(EditParseError::InvalidState {
                    state: "InSearch but current_search is None".to_string(),
                })
            }
        }
    }

    fn handle_replace(&mut self, line: &str, _line_num: usize) -> Result<(), EditParseError> {
        if line.starts_with(">>>>>>> REPLACE") || line.starts_with(">>>>>>> INSERT") {
            // Both REPLACE and INSERT markers end the block
            let search = self.current_search.take().unwrap_or_default();
            let replacement = self.current_replace.take().unwrap_or_default();

            self.edits.push(EditBlock {
                search,
                replacement,
                operation: EditOperation::Replace, // Will be inferred later
            });

            self.state = ParseState::Start;
            Ok(())
        } else {
            // Add line to replacement block (including empty lines — they
            // represent intentional blank lines in the replacement content).
            self.current_replace
                .get_or_insert_with(Vec::new)
                .push(line.to_string());
            Ok(())
        }
    }

    fn finish(mut self) -> Result<Vec<EditBlock>, EditParseError> {
        // Validate final state
        if self.state != ParseState::Start {
            return Err(EditParseError::UnterminatedBlock);
        }

        // Validate and infer operation types
        for edit in &mut self.edits {
            // Validate: both empty is not allowed
            if edit.search.is_empty() && edit.replacement.is_empty() {
                return Err(EditParseError::EmptyBlock);
            }

            // Infer Insert operation: empty SEARCH with non-empty REPLACE
            if edit.operation == EditOperation::Replace
                && edit.search.is_empty()
                && !edit.replacement.is_empty()
            {
                edit.operation = EditOperation::Insert;
            }
        }

        Ok(self.edits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    Start,
    InSearch,
    InReplace,
}

impl Command {
    /// Parse a command reference from format: [command: cmd](#href)
    /// Returns None if the format doesn't match
    pub fn parse(input: &str) -> Option<Self> {
        // Format: [command: cmd](#href)
        // Must start with "[command:" and end with "]"
        let input = input.trim();

        // Check if it matches [command: ...] pattern
        if !input.starts_with("[command:") {
            return None;
        }

        // Find the closing ] of [command: ...]
        let first_bracket_end = input.find(']')?;
        let first_part = &input[..=first_bracket_end];

        // Extract command name from [command: name]
        let name = first_part.strip_prefix("[command:")?.strip_suffix(']')?.trim().to_string();

        // After ] there should be (#href)
        let remaining = &input[first_bracket_end + 1..];
        let remaining = remaining.trim();

        if !remaining.starts_with("(#") {
            return None;
        }

        // Find closing )
        let paren_end = remaining.find(')')?;
        if paren_end == 0 {
            return None;
        }

        // Extract href (without # prefix)
        let href_part = &remaining[2..paren_end]; // Skip "(#"
        let href = href_part.to_string();

        Some(Command { name, href })
    }
}

/// Represents a txtar archive containing multiple files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    /// Comment lines before the first file
    pub comment: String,
    /// Commands extracted from comment section
    pub commands: Vec<Command>,
    /// Files in the archive
    pub files: Vec<File>,
    /// Command index cache for O(1) lookup by href
    /// (Not included in PartialEq/Eq comparisons)
    command_index: std::collections::HashMap<String, usize>,
}

impl Default for Archive {
    fn default() -> Self {
        Self {
            comment: String::default(),
            commands: Vec::default(),
            files: Vec::default(),
            command_index: std::collections::HashMap::default(),
        }
    }
}

/// Error for snippet reference validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetRefError {
    /// File name with the invalid reference
    pub file: String,
    /// Missing command href
    pub missing_command: String,
}

impl Archive {
    /// Create a new empty archive
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an archive with a comment
    pub fn with_comment(comment: impl Into<String>) -> Self {
        Self {
            comment: comment.into(),
            ..Default::default()
        }
    }

    /// Add a file to the archive
    /// Returns an error if a normal file (non-snippet, non-edit) with the same name already exists
    pub fn add_file(&mut self, file: File) -> anyhow::Result<()> {
        // Check for duplicates only for normal files (not snippet/edit references)
        if file.snippet_ref.is_none() && file.edit_ref.is_none() {
            if self.files.iter().any(|f| f.name == file.name && f.snippet_ref.is_none() && f.edit_ref.is_none()) {
                anyhow::bail!("Duplicate file: {}", file.name);
            }
        }
        self.files.push(file);
        Ok(())
    }

    /// Add a file from a path
    pub fn add_file_from_path(&mut self, path: &Path, archive_name: Option<String>) -> anyhow::Result<()> {
        let data = std::fs::read(path)?;

        // Use provided name or the file's actual name
        let name = archive_name.unwrap_or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        self.add_file(File::new(name, data))?;
        Ok(())
    }

    /// Parse command references from the comment section
    /// Looks for patterns like [command: cmd](#href) in markdown link format
    pub fn parse_commands(&mut self) {
        self.commands.clear();

        // Look for markdown-style links: [command: cmd](#href)
        // We need to find these in the comment text
        let text = self.comment.clone();

        // Simple regex-like search for [command: ...](#...) pattern
        let mut chars = text.chars().peekable();
        let mut result = Vec::new();

        while let Some(c) = chars.next() {
            if c == '[' {
                // Try to parse a command reference
                let mut remaining = String::from("[");
                while let Some(&next_c) = chars.peek() {
                    if next_c == '\n' {
                        break; // Don't span multiple lines
                    }
                    // Safety: We just peeked at next_c, so next() should succeed
                    if let Some(ch) = chars.next() {
                        remaining.push(ch);
                    }
                    if next_c == ')' {
                        break;
                    }
                }

                if let Some(cmd) = Command::parse(&remaining) {
                    result.push(cmd);
                }
            }
        }

        self.commands = result;

        // Rebuild command index after parsing
        self.rebuild_command_index();
    }

    /// Rebuild the command index cache
    /// Call this after modifying the commands list
    fn rebuild_command_index(&mut self) {
        self.command_index.clear();
        for (i, cmd) in self.commands.iter().enumerate() {
            self.command_index.insert(cmd.href.clone(), i);
        }
    }

    /// Get a command by its href (O(1) lookup using cached index)
    pub fn get_command(&self, href: &str) -> Option<&Command> {
        self.command_index.get(href)
            .and_then(|&idx| self.commands.get(idx))
    }

    /// Validate that all snippet references point to existing commands
    /// Returns Ok with empty vec if all valid, Err with list of errors otherwise
    pub fn validate_snippet_refs(&self) -> Result<Vec<SnippetRefError>, Vec<SnippetRefError>> {
        let mut errors = Vec::new();

        for file in &self.files {
            if let Some(ref_obj) = &file.snippet_ref {
                if let Some(href) = &ref_obj.command_href {
                    // Use cached index for O(1) lookup instead of O(n) HashSet
                    if self.command_index.get(href).is_none() {
                        errors.push(SnippetRefError {
                            file: file.name.clone(),
                            missing_command: href.clone(),
                        });
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(errors)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_needs_binary_encoding_utf8() {
        let file = File::new("normal.txt", "hello 世界");
        assert!(!file.is_binary);
    }

    #[test]
    fn test_file_needs_binary_encoding_binary() {
        let file = File::new("image.jpg", &[0xFF, 0xD8, 0xFF, 0xE0]);
        assert!(file.is_binary);
    }

    #[test]
    fn test_archive_name() {
        let text_file = File::new("test.txt", "hello");
        assert_eq!(text_file.archive_name(), "test.txt");

        let binary_file = File::with_encoding("image.jpg", vec![0xFF, 0xD8], true);
        assert_eq!(binary_file.archive_name(), "image.jpg[.base64]");
    }

    #[test]
    fn test_parse_archive_name() {
        assert_eq!(
            File::parse_archive_name("test.txt"),
            ("test.txt".to_string(), false)
        );
        assert_eq!(
            File::parse_archive_name("image.jpg[.base64]"),
            ("image.jpg".to_string(), true)
        );
    }

    #[test]
    fn test_content_marker_detection() {
        // File with marker pattern in content should be binary
        let content = r#"This is a file
-- some_file.txt --
with marker pattern"#;
        let file = File::new("test.txt", content);
        assert!(file.is_binary);
        assert_eq!(file.binary_reason, Some(BinaryReason::ContentConflict));
    }

    #[test]
    fn test_content_marker_detection_empty_marker() {
        // Empty markers should not trigger binary
        let content = r#"This is a file
--   --
with empty marker"#;
        let file = File::new("test.txt", content);
        // Empty marker (only spaces) should not trigger binary
        assert!(!file.is_binary);
    }

    #[test]
    fn test_encoding_detection_text() {
        let data = "Hello, world!".as_bytes();
        let detection = File::detect_encoding("test.txt", data, &EncodingConfig::default());
        assert!(matches!(detection, EncodingDetection::Text { .. }));
    }

    #[test]
    fn test_encoding_detection_binary_utf8() {
        let data = b"\xFF\xFE\xFD";
        let detection = File::detect_encoding("test.txt", data, &EncodingConfig::default());
        assert!(matches!(detection, EncodingDetection::Binary { reason: BinaryReason::InvalidUtf8 }));
    }

    #[test]
    fn test_encoding_detection_content_conflict() {
        let data = b"-- file.txt --\ncontent";
        let detection = File::detect_encoding("test.txt", data, &EncodingConfig::default());
        assert!(matches!(detection, EncodingDetection::Binary { reason: BinaryReason::ContentConflict }));
    }

    #[test]
    fn test_encoding_config_disable_content_check() {
        let data = b"-- file.txt --\ncontent";
        let config = EncodingConfig {
            check_content_markers: false,
            validate_utf8: true,
        };
        let detection = File::detect_encoding("test.txt", data, &config);
        // Should not detect content conflict when disabled
        assert!(matches!(detection, EncodingDetection::Text { .. }));
    }

    #[test]
    fn test_encoding_config_disable_utf8_check() {
        let data = b"\xFF\xFE\xFD";
        let config = EncodingConfig {
            check_content_markers: true,
            validate_utf8: false,
        };
        let detection = File::detect_encoding("test.txt", data, &config);
        // Should not detect invalid UTF-8 when disabled
        assert!(matches!(detection, EncodingDetection::Text { .. }));
    }

    // Tests for Command parsing
    #[test]
    fn test_command_parse_simple() {
        let input = "[command: rg](#search1)";
        let cmd = Command::parse(input).unwrap();
        assert_eq!(cmd.name, "rg");
        assert_eq!(cmd.href, "search1");
    }

    #[test]
    fn test_command_parse_with_spaces() {
        let input = "[command: rg ](#search2)";
        let cmd = Command::parse(input).unwrap();
        assert_eq!(cmd.name, "rg");
        assert_eq!(cmd.href, "search2");
    }

    #[test]
    fn test_command_parse_complex_name() {
        let input = "[command: git diff](#change1)";
        let cmd = Command::parse(input).unwrap();
        assert_eq!(cmd.name, "git diff");
        assert_eq!(cmd.href, "change1");
    }

    #[test]
    fn test_command_parse_invalid_no_href() {
        let input = "[command: rg]";
        assert!(Command::parse(input).is_none());
    }

    #[test]
    fn test_command_parse_invalid_no_hash() {
        let input = "[command: rg](search1)";
        assert!(Command::parse(input).is_none());
    }

    // Tests for SnippetRef parsing
    #[test]
    fn test_snippet_ref_parse_simple() {
        let input = "[.snippet:42]";
        let ref_obj = SnippetRef::parse(input).unwrap();
        assert!(ref_obj.command_href.is_none());
        assert_eq!(ref_obj.line, 42);
    }

    #[test]
    fn test_snippet_ref_parse_with_full_href() {
        let input = "[.snippet#search1:10]";
        let ref_obj = SnippetRef::parse(input).unwrap();
        assert_eq!(ref_obj.command_href.as_deref(), Some("search1"));
        assert_eq!(ref_obj.line, 10);
    }

    #[test]
    fn test_snippet_ref_parse_with_shorthand_href() {
        let input = "[.#search1:10]";
        let ref_obj = SnippetRef::parse(input).unwrap();
        assert_eq!(ref_obj.command_href.as_deref(), Some("search1"));
        assert_eq!(ref_obj.line, 10);
    }

    #[test]
    fn test_snippet_ref_parse_invalid_no_bracket() {
        let input = ".snippet:42";
        assert!(SnippetRef::parse(input).is_err());
    }

    #[test]
    fn test_snippet_ref_parse_invalid_no_dot_hash() {
        let input = "search1:10";
        assert!(SnippetRef::parse(input).is_err());
    }

    // New tests for error reporting
    #[test]
    fn test_snippet_ref_parse_error_invalid_format() {
        let input = "invalid";
        let err = SnippetRef::parse(input).unwrap_err();
        assert!(matches!(err, SnippetParseError::InvalidFormat));
    }

    #[test]
    fn test_snippet_ref_parse_error_missing_bracket() {
        let input = "[.snippet:42";
        let err = SnippetRef::parse(input).unwrap_err();
        assert!(matches!(err, SnippetParseError::MissingClosingBracket));
    }

    // EditRef::apply() tests
    #[test]
    fn test_edit_apply_single_line_replace() {
        let content = "line 1\nline 2\nline 3";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["line 2".to_string()],
                    replacement: vec!["modified line 2".to_string()],
                    operation: EditOperation::Replace,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "line 1\nmodified line 2\nline 3");
    }

    #[test]
    fn test_edit_apply_multi_line_replace() {
        let content = "line 1\nline 2\nline 3\nline 4";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["line 2".to_string(), "line 3".to_string()],
                    replacement: vec!["new line 2".to_string(), "new line 3".to_string()],
                    operation: EditOperation::Replace,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "line 1\nnew line 2\nnew line 3\nline 4");
    }

    #[test]
    fn test_edit_apply_delete() {
        let content = "line 1\nline 2\nline 3";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["line 2".to_string()],
                    replacement: vec![],
                    operation: EditOperation::Delete,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "line 1\nline 3");
    }

    #[test]
    fn test_edit_apply_insert_at_beginning() {
        let content = "line 1\nline 2";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec![],
                    replacement: vec!["inserted line".to_string()],
                    operation: EditOperation::Insert,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "inserted line\nline 1\nline 2");
    }

    #[test]
    fn test_edit_apply_insert_to_empty() {
        let content = "";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec![],
                    replacement: vec!["first line".to_string()],
                    operation: EditOperation::Insert,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "first line");
    }

    #[test]
    fn test_edit_apply_multiple_edits_sequential() {
        let content = "line 1\nline 2\nline 3";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["line 2".to_string()],
                    replacement: vec!["modified 2".to_string()],
                    operation: EditOperation::Replace,
                },
                EditBlock {
                    search: vec!["line 3".to_string()],
                    replacement: vec!["modified 3".to_string()],
                    operation: EditOperation::Replace,
                },
            ],
        };

        let result = edit_ref.apply(content).unwrap();
        assert_eq!(result, "line 1\nmodified 2\nmodified 3");
    }

    #[test]
    fn test_edit_apply_search_not_found() {
        let content = "line 1\nline 2\nline 3";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["nonexistent".to_string()],
                    replacement: vec!["replacement".to_string()],
                    operation: EditOperation::Replace,
                },
            ],
        };

        let result = edit_ref.apply(content);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EditApplyError::SearchNotFound { .. }));
    }

    #[test]
    fn test_edit_apply_empty_content_error() {
        let content = "";
        let edit_ref = EditRef {
            command_href: None,
            start_line: None,
            edits: vec![
                EditBlock {
                    search: vec!["line 1".to_string()],
                    replacement: vec!["replacement".to_string()],
                    operation: EditOperation::Replace,
                },
            ],
        };

        let result = edit_ref.apply(content);
        assert!(matches!(result.unwrap_err(), EditApplyError::EmptyContent));
    }
}
