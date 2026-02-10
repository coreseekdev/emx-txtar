# emx-txtar - AI Development Guide

## Project Overview

**emx-txtar** is a Rust library implementing the txtar archive format with binary file support. This document provides context for AI assistants working on this codebase.

## Architecture

### Core Components

1. **Archive** (`src/archive.rs`)
   - Main data structure holding archive metadata and files
   - Provides methods for file management and edit operations
   - Handles content validation and duplicate detection

2. **Encoder** (`src/encoder.rs`)
   - Converts `Archive` to txtar text format
   - Implements proper formatting and escaping
   - Handles binary file base64 encoding

3. **Decoder** (`src/decoder.rs`)
   - Parses txtar text format into `Archive`
   - Validates format and detects content types
   - Supports edit references and snippet extraction

4. **File** (`src/archive.rs`)
   - Represents individual files in the archive
   - Tracks encoding, content, and metadata

## Key Design Decisions

### Binary File Detection

Binary files are detected through content analysis:
- UTF-8 validation
- Presence of `-- xxxx --` marker patterns (conflict markers)
- Automatic `.base64` suffix for binary files

### Edit References

Supports snippet references from other archives:
```text
[edit:path/to/other.txtar:section]
```

This allows composite archives and code reuse.

### Error Handling

Uses `anyhow` for error propagation:
- Parse errors include line/col information
- Clear error messages for format violations
- Validation errors indicate specific problems

## Testing Strategy

### Unit Tests

Located in `src/` modules:
- Encoding/decoding round-trips
- Edge cases (empty files, special characters)
- Binary content detection
- Edit operations

### Example Tests

In `examples/` directory:
- `txtar_basic.rs` - Basic usage examples
- `content_detection.rs` - Binary detection examples

Run examples:
```bash
cargo run --example txtar_basic
cargo run --example content_detection
```

## Common Tasks

### Adding New Features

1. **New file metadata**: Extend `File` struct
2. **New encoding**: Add to `FileEncoding` enum
3. **New validation**: Add to `Decoder::validate_file`

### Format Extensions

When extending the format:
1. Update parser in `decoder.rs`
2. Update encoder in `encoder.rs`
3. Add tests for round-trip behavior
4. Document in README.md

## Dependencies

- **anyhow** - Error handling
- **base64** - Binary file encoding
- **serde** (dev) - Optional serialization support

Minimize dependencies - this is a foundational library.

## Performance Considerations

- Encoding/decoding is synchronous (no async)
- Files are loaded entirely into memory
- For large archives, consider streaming approach

## Code Style

- Use `Result<T>` for fallible operations
- Provide clear error messages with context
- Include doc examples for public APIs
- Use `#[cfg(test)]` for test-only code

## Testing with AI

When running tests:
```bash
cargo test
```

Expected output: 62 tests passing (61 unit + 1 doc)

## Known Limitations

1. **Memory** - All files loaded into memory
2. **Streaming** - No streaming support for large files
3. **Compression** - No built-in compression
4. **Signatures** - No cryptographic signing

## Future Enhancements

Potential improvements:
- [ ] Streaming API for large archives
- [ ] Compression support (gzip, zstd)
- [ ] Cross-platform line ending handling
- [ ] Parallel encoding/decoding

## See Also

- [Txtar Format Spec](https://pkg.go.dev/golang.org/x/tools/txtar)
- [emx-testspec](https://github.com/coreseekdev/emx-testspec) - Uses txtar for test fixtures
- [Go Implementation](https://github.com/golang/tools/blob/master/txtar/archive.go)
