//! Example of encoding and decoding a txtar archive

use emx_txtar::{archive::{Archive, File}, decoder::Decoder, encoder::Encoder};

fn main() -> anyhow::Result<()> {
    println!("=== Txtar Archive Example ===\n");

    // Create an archive with text and binary files
    let mut archive = Archive::with_comment("Example txtar archive\n");

    // Add text file
    archive.add_file(File::new("README.md", "# Example Archive\n\nThis is a sample file."));

    // Add binary file (simulated JPEG header)
    let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    archive.add_file(File::with_encoding("image.jpg", jpeg_header, true));

    // Add file with conflict pattern in name (auto-detected as binary)
    archive.add_file(File::new("-- weird --.txt", b"This filename has conflict pattern"));

    // Encode archive
    let encoder = Encoder::new();
    let encoded = encoder.encode(&archive)?;

    println!("Encoded archive:");
    println!("---");
    println!("{}", encoded);
    println!("---");

    // Decode archive
    let decoder = Decoder::new().with_verbose(1);
    let decoded = decoder.decode(&encoded)?;

    println!("\nDecoded {} files:", decoded.files.len());
    for file in &decoded.files {
        println!("  - {} ({} bytes, binary: {})",
            file.name,
            file.data.len(),
            file.is_binary
        );
    }

    // Verify round-trip
    assert_eq!(archive.files.len(), decoded.files.len());
    for (orig, dec) in archive.files.iter().zip(decoded.files.iter()) {
        assert_eq!(orig.name, dec.name);
        assert_eq!(orig.is_binary, dec.is_binary);

        // For non-binary files, the decoder preserves trailing newline behavior
        if orig.is_binary {
            assert_eq!(orig.data, dec.data);
        } else {
            // Text files may differ by trailing newline - normalize comparison
            let orig_data = String::from_utf8(orig.data.clone()).unwrap();
            let dec_data = String::from_utf8(dec.data.clone()).unwrap();
            assert_eq!(orig_data.trim_end(), dec_data.trim_end());
        }
    }

    println!("\nRound-trip verification passed!");

    Ok(())
}
