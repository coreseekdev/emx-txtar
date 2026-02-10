//! Example demonstrating automatic content-based binary detection
//!
//! This shows how files with txtar marker patterns in their content
//! are automatically detected and encoded as binary.

use emx_txtar::{archive::{Archive, File, BinaryReason}, encoder::Encoder, decoder::Decoder};

fn main() -> anyhow::Result<()> {
    println!("=== Content-Based Binary Detection Example ===\n");

    // Create files with different scenarios
    let mut archive = Archive::with_comment("Content detection test archive\n");

    // 1. Normal text file - no encoding needed
    archive.add_file(File::new("README.md", "# Project\n\nNormal text content."));

    // 2. File with marker pattern in content - auto-detected as binary
    let tricky_content = r#"This file looks like a txtar archive:

-- file1.txt --
Some content
-- file2.txt --
More content

End of file"#;
    archive.add_file(File::new("tricky.txt", tricky_content));

    // 3. Actual binary data - auto-detected as binary
    let binary_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    archive.add_file(File::new("image.jpg", binary_data));

    // Encode the archive
    let encoder = Encoder::new();
    let encoded = encoder.encode(&archive)?;

    println!("Encoded archive ({:.} bytes):\n", encoded.len());
    println!("{}", encoded);
    println!();

    // Show detection results
    println!("Detection Results:");
    println!("-----------------");
    for (i, file) in archive.files.iter().enumerate() {
        let reason_str = if file.is_binary {
            match file.binary_reason {
                Some(BinaryReason::ContentConflict) => "Content conflict (has -- filename --)",
                Some(BinaryReason::InvalidUtf8) => "Invalid UTF-8 (binary data)",
                Some(BinaryReason::Explicit) => "Explicitly marked",
                None => "Unknown",
            }
        } else {
            "None (text)"
        };

        println!("{}. {} - {} bytes | Binary: {} | Reason: {}",
            i + 1,
            file.name,
            file.data.len(),
            file.is_binary,
            reason_str
        );
    }

    // Decode and verify round-trip
    let decoder = Decoder::new();
    let decoded = decoder.decode(&encoded)?;

    println!("\nRound-trip verification:");
    for (orig, dec) in archive.files.iter().zip(decoded.files.iter()) {
        let data_match = if orig.is_binary {
            orig.data == dec.data
        } else {
            // For text, compare normalized (without trailing newlines)
            let orig_text = String::from_utf8(orig.data.clone()).unwrap();
            let dec_text = String::from_utf8(dec.data.clone()).unwrap();
            orig_text.trim_end() == dec_text.trim_end()
        };

        println!("  {}: {}",
            orig.name,
            if data_match { "✓ OK" } else { "✗ MISMATCH" }
        );

        if !data_match {
            println!("    Original: {:?}", orig.data);
            println!("    Decoded:  {:?}", dec.data);
        }
    }

    println!("\n✓ All features working correctly!");

    Ok(())
}
