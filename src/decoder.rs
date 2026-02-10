//! Txtar archive decoder

use crate::archive::{Archive, File, SnippetRef, EditRef};
use anyhow::{anyhow, Result};
use base64::Engine;

// Re-export constants from archive module
use crate::archive::{MARKER_PREFIX, MARKER_SUFFIX, MARKER_PREFIX_LEN, MARKER_SUFFIX_LEN, BASE64_SUFFIX};

// Binary data constants
const BINARY_NEWLINE: u8 = b'\n';
const BINARY_CARRIAGE_RETURN: u8 = b'\r';

/// Decodes a txtar archive
pub struct Decoder {
    /// Verbosity level for conflict detection warnings
    verbose: u8,
}

impl Decoder {
    /// Create a new decoder
    pub fn new() -> Self {
        Self { verbose: 0 }
    }

    /// Set verbosity level (0-3)
    pub fn with_verbose(mut self, level: u8) -> Self {
        self.verbose = level;
        self
    }

    /// Create a File from accumulated data, handling binary decoding
    fn create_file_from_data(&self, name: String, is_binary: bool, data: Vec<u8>) -> Result<File> {
        if is_binary {
            // Decode base64 data
            let base64_str = Self::filter_base64_data(&data);
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&base64_str)
                .map_err(|e| anyhow!("Failed to decode base64 for file '{}': {}", name, e))?;
            Ok(File::with_encoding(name, decoded, true))
        } else {
            // Remove trailing newline if present
            let mut data = data;
            if data.ends_with(b"\n") {
                data.pop();
            }
            Ok(File::with_encoding(name, data, false))
        }
    }

    /// Filter base64 data by removing newlines and carriage returns
    fn filter_base64_data(data: &[u8]) -> String {
        data.iter()
            .filter(|&&c| c != BINARY_NEWLINE && c != BINARY_CARRIAGE_RETURN)
            .map(|&c| c as char)
            .collect()
    }

    /// Decode a txtar archive from a string
    pub fn decode(&self, input: &str) -> Result<Archive> {
        let mut archive = Archive::new();
        let mut current_file: Option<(String, bool, Option<SnippetRef>, Option<EditRef>, Vec<u8>)> = None;

        for (_line_num, line) in input.lines().enumerate() {
            // Check for file marker
            if let Some((name, is_binary, snippet_ref, edit_ref)) = self.parse_file_marker(line) {
                // Save previous file using helper method
                if let Some((name, is_binary, snippet_ref, edit_ref, data)) = current_file.take() {
                    let mut file = self.create_file_from_data(name, is_binary, data)?;
                    file.snippet_ref = snippet_ref;
                    file.edit_ref = edit_ref;
                    archive.add_file(file)?;
                }

                // Start new file
                current_file = Some((name, is_binary, snippet_ref, edit_ref, Vec::new()));
                continue;
            }

            // Add content to current file
            if let Some((_, is_binary, _, _, ref mut data)) = current_file {
                if is_binary {
                    // Accumulate base64 lines
                    if !line.trim().is_empty() {
                        data.extend_from_slice(line.as_bytes());
                        data.push(BINARY_NEWLINE);
                    }
                } else {
                    // Text content
                    data.extend_from_slice(line.as_bytes());
                    data.push(BINARY_NEWLINE);
                }
            } else {
                // Before first file - this is comment
                if !line.trim().is_empty() {
                    if !archive.comment.is_empty() {
                        archive.comment.push('\n');
                    }
                    archive.comment.push_str(line);
                }
            }
        }

        // Save last file using helper method
        if let Some((name, is_binary, snippet_ref, edit_ref, data)) = current_file.take() {
            let mut file = self.create_file_from_data(name, is_binary, data)?;
            file.snippet_ref = snippet_ref;
            file.edit_ref = edit_ref;
            archive.add_file(file)?;
        }

        // Parse commands from comment section
        archive.parse_commands();

        // Parse edit blocks and validate file existence
        self.parse_and_validate_edits(&mut archive)?;

        Ok(archive)
    }

    /// Parse a file marker line like "-- filename --" or "-- filename[.base64] --"
    /// Also handles snippet references like "-- filename[.snippet:N] --" or "-- filename[.#href:line] --"
    /// And edit references like "-- filename[.edit] --" or "-- filename[.edit#href:line] --"
    fn parse_file_marker(&self, line: &str) -> Option<(String, bool, Option<SnippetRef>, Option<EditRef>)> {
        let trimmed = line.trim();

        // Must start with "-- " and end with " --"
        if !trimmed.starts_with(MARKER_PREFIX) || !trimmed.ends_with(MARKER_SUFFIX) {
            return None;
        }

        // Extract the name between the markers
        let name_part = &trimmed[MARKER_PREFIX_LEN..trimmed.len() - MARKER_SUFFIX_LEN];

        // Parse filename with all bracket-enclosed tags
        let (filename, is_binary, snippet_ref, edit_ref) = Self::parse_name_and_tags(name_part);

        // Check for filename conflicts (only if not already marked as binary)
        if !is_binary && self.check_filename_conflict(&filename) {
            if self.verbose > 0 {
                eprintln!("Warning: Filename '{}' contains txtar marker pattern, but is not marked as binary", filename);
            }
        }

        Some((filename, is_binary, snippet_ref, edit_ref))
    }

    /// Parse filename with optional bracket-enclosed tags
    /// Handles formats like: filename, filename[.base64], filename[.snippet:N],
    /// filename[.base64][.snippet:N], filename[.#href:line], filename[.edit], etc.
    fn parse_name_and_tags(name_part: &str) -> (String, bool, Option<SnippetRef>, Option<EditRef>) {
        let mut is_binary = false;
        let mut snippet_ref = None;
        let mut edit_ref = None;

        // Find the base filename (before first bracket)
        let base_name = if let Some(bracket_start) = name_part.find('[') {
            &name_part[..bracket_start]
        } else {
            return (name_part.trim().to_string(), false, None, None);
        };

        // Process each bracket-enclosed tag
        let mut rest = &name_part[base_name.len()..];
        while let Some(bracket_end) = rest.find(']') {
            let tag = &rest[..=bracket_end]; // Include the closing bracket

            // Check for base64 tag
            if tag == BASE64_SUFFIX {
                is_binary = true;
            }
            // Check for snippet reference tags
            else if let Ok(ref_obj) = SnippetRef::parse(tag) {
                snippet_ref = Some(ref_obj);
            }
            // Check for edit reference tags
            else if let Some((href, start_line)) = Self::parse_edit_tag(tag) {
                edit_ref = Some(EditRef {
                    command_href: href,
                    start_line,
                    edits: Vec::new(), // Will be parsed later from file content
                });
            }

            // Move to next tag
            rest = &rest[bracket_end + 1..];
        }

        (base_name.trim().to_string(), is_binary, snippet_ref, edit_ref)
    }

    /// Parse an edit tag like [.edit] or [.edit#href:line]
    fn parse_edit_tag(tag: &str) -> Option<(Option<String>, Option<usize>)> {
        // Try [.edit#href:line] format
        if tag.starts_with("[.edit#") {
            let content = tag.strip_prefix("[.edit#")?;
            let end_bracket = content.find(']')?;
            let inner = &content[..end_bracket];

            let colon_pos = inner.find(':')?;
            let href = inner[..colon_pos].to_string();
            let line_str = &inner[colon_pos + 1..];
            let line = line_str.parse::<usize>().ok()?;
            return Some((Some(href), Some(line)));
        }

        // Try [.edit] format
        if tag == "[.edit]" {
            return Some((None, None));
        }

        None
    }

    /// Check if a filename conflicts with txtar marker pattern
    fn check_filename_conflict(&self, name: &str) -> bool {
        name.contains("-- ") && name.contains(" --")
    }

    /// Parse edit blocks from files and validate file existence
    fn parse_and_validate_edits(&self, archive: &mut Archive) -> Result<()> {
        // First, collect files that need validation
        let files_to_process: Vec<(usize, String)> = archive.files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.edit_ref.as_ref().map(|_er| (i, f.name.clone())))
            .collect();

        // Validate file existence first (before any modifications)
        for (_, filename) in &files_to_process {
            self.validate_file_exists_for_edit(archive, filename)?;
        }

        // Then parse edit blocks
        for (idx, _) in files_to_process {
            let file = &mut archive.files[idx];
            // Safety: We filtered files to only include those with edit_ref
            let _edit_ref = file.edit_ref.as_ref()
                .expect("edit_ref should be Some (filtered by filter_map)");

            // Parse edit blocks from file content
            let content = std::str::from_utf8(&file.data)
                .map_err(|_| anyhow!("File '{}' is not valid UTF-8", file.name))?;
            let edits = EditRef::parse_content(content)
                .map_err(|e| anyhow!("Failed to parse edit blocks in '{}': {}", file.name, e))?;

            // Update file with parsed edits
            if let Some(er) = &mut file.edit_ref {
                er.edits = edits;
            }
        }

        Ok(())
    }

    /// Validate that the target file exists (in txtar or filesystem)
    fn validate_file_exists_for_edit(&self, archive: &Archive, filename: &str) -> Result<()> {
        // Check if file exists in txtar (as non-edit file)
        let exists_in_txtar = archive.files.iter()
            .any(|f| f.name == filename && f.edit_ref.is_none());

        // Check if file exists in filesystem
        let exists_on_fs = std::path::Path::new(filename).exists();

        if !exists_in_txtar && !exists_on_fs {
            Err(anyhow!(
                "Edit target file '{}' not found in archive or filesystem (at least one must exist)",
                filename
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::EditOperation;

    #[test]
    fn test_decode_simple_text() {
        let input = r#"-- file1.txt --
Hello, world!"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 1);
        assert_eq!(archive.files[0].name, "file1.txt");
        assert_eq!(archive.files[0].data, b"Hello, world!");
        assert!(!archive.files[0].is_binary);
    }

    #[test]
    fn test_decode_binary() {
        let input = r#"-- image.jpg[.base64] --
/9j/"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 1);
        assert_eq!(archive.files[0].name, "image.jpg");
        assert_eq!(archive.files[0].data, vec![0xFF, 0xD8, 0xFF]);
        assert!(archive.files[0].is_binary);
    }

    #[test]
    fn test_decode_multiple_files() {
        let input = r#"-- file1.txt --
Content 1
-- file2.txt --
Content 2"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 2);
        assert_eq!(archive.files[0].name, "file1.txt");
        assert_eq!(archive.files[1].name, "file2.txt");
    }

    #[test]
    fn test_decode_with_comment() {
        let input = r#"This is a comment
Another comment line

-- file.txt --
Content"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert!(archive.comment.contains("This is a comment"));
        assert_eq!(archive.files.len(), 1);
    }

    #[test]
    fn test_decode_with_subdirectories() {
        let input = r#"-- dir/subdir/file.txt --
Content"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[0].name, "dir/subdir/file.txt");
    }

    #[test]
    fn test_decode_with_snippet_ref() {
        let input = r#"-- file.txt[.snippet:42] --
Content of file"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[0].name, "file.txt");
        assert_eq!(archive.files[0].data, b"Content of file");
        assert!(archive.files[0].snippet_ref.is_some());
        let ref_obj = archive.files[0].snippet_ref.as_ref().unwrap();
        assert!(ref_obj.command_href.is_none());
        assert_eq!(ref_obj.line, 42);
    }

    #[test]
    fn test_decode_with_command_ref_shorthand() {
        let input = r#"-- file.txt[.#search1:10] --
Content of file"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[0].name, "file.txt");
        assert_eq!(archive.files[0].data, b"Content of file");
        assert!(archive.files[0].snippet_ref.is_some());
        let ref_obj = archive.files[0].snippet_ref.as_ref().unwrap();
        assert_eq!(ref_obj.command_href.as_deref(), Some("search1"));
        assert_eq!(ref_obj.line, 10);
    }

    #[test]
    fn test_decode_with_command_ref_full() {
        let input = r#"-- file.txt[.snippet#search1:10] --
Content of file"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[0].name, "file.txt");
        assert_eq!(archive.files[0].data, b"Content of file");
        assert!(archive.files[0].snippet_ref.is_some());
        let ref_obj = archive.files[0].snippet_ref.as_ref().unwrap();
        assert_eq!(ref_obj.command_href.as_deref(), Some("search1"));
        assert_eq!(ref_obj.line, 10);
    }

    #[test]
    fn test_decode_with_commands_in_comment() {
        let input = r#"This is a commit block with command references:
[command: rg](#search1)
[command: git diff](#change1)

-- file.txt[.#search1:10] --
Content"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        // Check commands were parsed
        assert_eq!(archive.commands.len(), 2);
        assert_eq!(archive.commands[0].name, "rg");
        assert_eq!(archive.commands[0].href, "search1");
        assert_eq!(archive.commands[1].name, "git diff");
        assert_eq!(archive.commands[1].href, "change1");

        // Check file references command
        assert_eq!(archive.files[0].name, "file.txt");
        assert!(archive.files[0].snippet_ref.is_some());
        let ref_obj = archive.files[0].snippet_ref.as_ref().unwrap();
        assert_eq!(ref_obj.command_href.as_deref(), Some("search1"));
    }

    #[test]
    fn test_decode_binary_with_snippet_ref() {
        let input = r#"-- image.jpg[.base64][.snippet:100] --
/9j/"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[0].name, "image.jpg");
        assert!(archive.files[0].is_binary);
        assert!(archive.files[0].snippet_ref.is_some());
        let ref_obj = archive.files[0].snippet_ref.as_ref().unwrap();
        assert!(ref_obj.command_href.is_none());
        assert_eq!(ref_obj.line, 100);
    }

    #[test]
    fn test_decode_duplicate_snippet_files_allowed() {
        let input = r#"-- file.txt[.snippet:10] --
First snippet

-- file.txt[.snippet:42] --
Second snippet"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 2);
        assert_eq!(archive.files[0].name, "file.txt");
        assert_eq!(archive.files[1].name, "file.txt");
        assert!(archive.files[0].snippet_ref.is_some());
        assert!(archive.files[1].snippet_ref.is_some());
    }

    #[test]
    fn test_decode_duplicate_normal_files_not_allowed() {
        let input = r#"-- file.txt --
Content 1

-- file.txt --
Content 2"#;

        let decoder = Decoder::new();
        let result = decoder.decode(input);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate file"));
    }

    #[test]
    fn test_decode_invalid_command_reference_warning() {
        let input = r#"-- file.txt[.#nonexistent:10] --
Content"#;

        let decoder = Decoder::new().with_verbose(1);
        let archive = decoder.decode(input).unwrap();

        // Should succeed but the invalid reference is noted
        assert_eq!(archive.files.len(), 1);
        assert!(archive.files[0].snippet_ref.is_some());
    }

    #[test]
    fn test_decode_valid_command_reference() {
        let input = r#"[command: rg](#search1)

-- file.txt[.#search1:10] --
Content"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.commands.len(), 1);
        assert_eq!(archive.commands[0].href, "search1");
        assert_eq!(archive.files[0].snippet_ref.as_ref().unwrap().command_href.as_deref(), Some("search1"));

        // Validation should pass
        assert!(archive.validate_snippet_refs().is_ok());
    }

    #[test]
    fn test_validate_snippet_refs_missing_command() {
        let input = r#"[command: rg](#search1)

-- file.txt[.#search2:10] --
Content"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        // Validation should fail
        let result = archive.validate_snippet_refs();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "file.txt");
        assert_eq!(errors[0].missing_command, "search2");
    }

    #[test]
    fn test_decode_edit_file_without_href() {
        let input = r#"-- target.txt --
original content

-- target.txt[.edit] --
<<<<<<< SEARCH
old line
=======
new line
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 2);
        assert_eq!(archive.files[1].name, "target.txt");
        assert!(archive.files[1].edit_ref.is_some());
        let edit_ref = archive.files[1].edit_ref.as_ref().unwrap();
        assert!(edit_ref.command_href.is_none());
        assert!(edit_ref.start_line.is_none());
        assert_eq!(edit_ref.edits.len(), 1);
        assert_eq!(edit_ref.edits[0].operation, EditOperation::Replace);
        assert_eq!(edit_ref.edits[0].search, vec!["old line"]);
        assert_eq!(edit_ref.edits[0].replacement, vec!["new line"]);
    }

    #[test]
    fn test_decode_edit_file_with_href() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit#cmd1:42] --
<<<<<<< SEARCH
old line
=======
new line
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files.len(), 2);
        assert_eq!(archive.files[1].name, "target.txt");
        assert!(archive.files[1].edit_ref.is_some());
        let edit_ref = archive.files[1].edit_ref.as_ref().unwrap();
        assert_eq!(edit_ref.command_href.as_deref(), Some("cmd1"));
        assert_eq!(edit_ref.start_line, Some(42));
        assert_eq!(edit_ref.edits.len(), 1);
    }

    #[test]
    fn test_decode_edit_delete_operation() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit] --
<<<<<<< SEARCH
line to delete
>>>>>>> DELETE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].operation, EditOperation::Delete);
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search, vec!["line to delete"]);
        assert!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].replacement.is_empty());
    }

    #[test]
    fn test_decode_edit_insert_operation() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit] --
<<<<<<< SEARCH
=======
new line to insert
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].operation, EditOperation::Insert);
        assert!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search.is_empty());
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].replacement, vec!["new line to insert"]);
    }

    #[test]
    fn test_decode_edit_multiple_blocks() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit] --
<<<<<<< SEARCH
first old
=======
first new
>>>>>>> REPLACE
<<<<<<< SEARCH
second old
=======
second new
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits.len(), 2);
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search, vec!["first old"]);
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[1].search, vec!["second old"]);
    }

    #[test]
    fn test_decode_edit_trailing_whitespace_ignored() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit] --
<<<<<<< SEARCH
line with spaces
=======
line with spaces
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        // Both search and replacement should have trailing whitespace trimmed
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search, vec!["line with spaces"]);
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].replacement, vec!["line with spaces"]);
    }

    #[test]
    fn test_decode_edit_multiline_blocks() {
        let input = r#"-- target.txt --
original

-- target.txt[.edit] --
<<<<<<< SEARCH
line 1
line 2
line 3
=======
new line 1
new line 2
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search, vec!["line 1", "line 2", "line 3"]);
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].replacement, vec!["new line 1", "new line 2"]);
    }

    #[test]
    fn test_decode_edit_target_exists_in_txtar() {
        let input = r#"-- target.txt --
original content

-- target.txt[.edit] --
<<<<<<< SEARCH
original
=======
modified
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let result = decoder.decode(input);

        // Should succeed - target file exists in txtar
        assert!(result.is_ok());
        let archive = result.unwrap();
        assert_eq!(archive.files.len(), 2);
    }

    #[test]
    fn test_decode_edit_target_missing_should_fail() {
        let input = r#"-- target.txt[.edit] --
<<<<<<< SEARCH
old
=======
new
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let result = decoder.decode(input);

        // Should fail - target file doesn't exist in txtar or filesystem
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found in archive or filesystem"));
    }

    #[test]
    fn test_decode_edit_file_can_duplicate_normal_file() {
        let input = r#"-- target.txt --
normal content

-- target.txt[.edit] --
<<<<<<< SEARCH
old
=======
new
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let result = decoder.decode(input);

        // Should succeed - .edit files can have same name as normal files
        assert!(result.is_ok());
        let archive = result.unwrap();
        assert_eq!(archive.files.len(), 2);
        assert_eq!(archive.files[0].name, "target.txt");
        assert_eq!(archive.files[1].name, "target.txt");
        assert!(archive.files[0].edit_ref.is_none());
        assert!(archive.files[1].edit_ref.is_some());
    }

    #[test]
    fn test_decode_edit_empty_search_with_replacement() {
        let input = r#"-- empty.txt --
(empty file)

-- empty.txt[.edit] --
<<<<<<< SEARCH
=======
inserted content
>>>>>>> REPLACE"#;

        let decoder = Decoder::new();
        let archive = decoder.decode(input).unwrap();

        // Should be parsed as Insert operation
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].operation, EditOperation::Insert);
        assert!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].search.is_empty());
        assert_eq!(archive.files[1].edit_ref.as_ref().unwrap().edits[0].replacement, vec!["inserted content"]);
    }
}
