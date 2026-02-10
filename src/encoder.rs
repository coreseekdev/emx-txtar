//! Txtar archive encoder

use crate::archive::{Archive, File};
use anyhow::Result;
use base64::Engine;

/// Encodes an archive into txtar format
pub struct Encoder {
    // Currently stateless, but reserved for future options
}

impl Encoder {
    /// Create a new encoder
    pub fn new() -> Self {
        Self {}
    }

    /// Encode an archive to a string
    pub fn encode(&self, archive: &Archive) -> Result<String> {
        let mut output = String::new();

        // Write comment if present
        if !archive.comment.is_empty() {
            output.push_str(&archive.comment);
            if !archive.comment.ends_with('\n') {
                output.push('\n');
            }
        }

        // Write each file
        for file in &archive.files {
            self.encode_file(&mut output, file)?;
        }

        Ok(output)
    }

    /// Encode a single file
    fn encode_file(&self, output: &mut String, file: &File) -> Result<()> {
        // Write file header
        output.push_str("-- ");
        output.push_str(&file.archive_name());
        output.push_str(" --\n");

        // Write file content
        let content = if file.is_binary {
            // Encode binary data as base64
            base64::engine::general_purpose::STANDARD.encode(&file.data)
        } else {
            // Use UTF-8 validation (should already be validated)
            std::str::from_utf8(&file.data)
                .map_err(|_| anyhow::anyhow!("File {} is not valid UTF-8 but not marked as binary", file.name))?
                .to_string()
        };

        output.push_str(&content);

        // Ensure trailing newline
        if !content.ends_with('\n') {
            output.push('\n');
        }

        Ok(())
    }

    /// Encode an archive directly to a writer
    pub fn encode_to_writer<W: std::io::Write>(&self, archive: &Archive, mut writer: W) -> Result<()> {
        let encoded = self.encode(archive)?;
        writer.write_all(encoded.as_bytes())?;
        Ok(())
    }

    /// Encode an archive to a file
    pub fn encode_to_file(&self, archive: &Archive, path: &std::path::Path) -> Result<()> {
        let encoded = self.encode(archive)?;
        std::fs::write(path, encoded)?;
        Ok(())
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_simple_text() {
        let mut archive = Archive::with_comment("Test archive\nComment\n");
        archive.add_file(File::new("file1.txt", "Hello, world!")).unwrap();

        let encoder = Encoder::new();
        let result = encoder.encode(&archive).unwrap();

        assert!(result.contains("Test archive"));
        assert!(result.contains("Comment"));
        assert!(result.contains("-- file1.txt --"));
        assert!(result.contains("Hello, world!"));
    }

    #[test]
    fn test_encode_binary() {
        let mut archive = Archive::new();
        archive.add_file(File::with_encoding("image.jpg", vec![0xFF, 0xD8, 0xFF], true)).unwrap();

        let encoder = Encoder::new();
        let result = encoder.encode(&archive).unwrap();

        assert!(result.contains("-- image.jpg[.base64] --"));
        // Base64 encoded version of [0xFF, 0xD8, 0xFF]
        assert!(result.contains("/9j/"));
    }

    #[test]
    fn test_encode_multiple_files() {
        let mut archive = Archive::new();
        archive.add_file(File::new("file1.txt", "Content 1")).unwrap();
        archive.add_file(File::new("file2.txt", "Content 2")).unwrap();

        let encoder = Encoder::new();
        let result = encoder.encode(&archive).unwrap();

        assert!(result.contains("-- file1.txt --"));
        assert!(result.contains("Content 1"));
        assert!(result.contains("-- file2.txt --"));
        assert!(result.contains("Content 2"));
    }

    #[test]
    fn test_encode_with_subdirectories() {
        let mut archive = Archive::new();
        archive.add_file(File::new("dir/subdir/file.txt", "Content")).unwrap();

        let encoder = Encoder::new();
        let result = encoder.encode(&archive).unwrap();

        assert!(result.contains("-- dir/subdir/file.txt --"));
        assert!(result.contains("Content"));
    }
}
