# emx-txtar

Txtar archive format support with binary file encoding for Rust.

## Overview

`emx-txtar` is a Rust implementation of the [txtar](https://pkg.go.dev/golang.org/x/tools/txtar) archive format, originally from Go's toolchain. It provides a simple text-based archive format ideal for:
- Test fixtures
- Configuration files
- Embedded resources
- Data interchange

## Features

- ✅ **Standard txtar format** - Compatible with Go's txtar implementation
- ✅ **Binary file support** - Automatic base64 encoding for non-UTF8 files
- ✅ **Content detection** - Smart detection of binary vs text content
- ✅ **Subdirectory support** - Files with paths like `dir/file.txt`
- ✅ **Edit operations** - Support for snippet references and file edits
- ✅ **Pure Rust** - No external dependencies beyond `anyhow` and `base64`
- ✅ **MIT License** - Free to use in any project

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
emx-txtar = "0.1"
```

Or use via Git:

```toml
[dependencies]
emx-txtar = { git = "https://github.com/coreseekdev/emx-txtar" }
```

## Usage

### Creating an Archive

```rust
use emx_txtar::{Archive, File};

let mut archive = Archive::new();
archive.add_file(File::new("README.md", b"# Hello World\n"));
archive.add_file(File::new("config.json", br#"{"key": "value"}"#));

let encoder = emx_txtar::Encoder::new();
let txtar_content = encoder.encode(&archive)?;
println!("{}", txtar_content);
```

### Parsing an Archive

```rust
use emx_txtar::Decoder;

let txtar_content = "-- README.md --
# Hello World

-- config.json --
{"key": "value"}
";

let decoder = Decoder::new();
let archive = decoder.decode(txtar_content.as_bytes())?;

for file in archive.files {
    println!("{}: {} bytes", file.name, file.content.len());
}
```

### Binary File Support

Binary files are automatically detected and encoded:

```rust
use emx_txtar::{Archive, File};

// Binary file - will be automatically base64 encoded
archive.add_file(File::with_encoding(
    "image.jpg",
    &[0xFF, 0xD8, 0xFF, 0xE0], // JPEG header
    true // is_binary
));
```

Output format:
```txtar
-- image.jpg --
[.base64]
/9j/4AAQSkZJRg==
```

### File Edit Operations

```rust
use emx_txtar::EditRef;

// Edit an existing file from another archive
let edit = EditRef::new(
    "README.md",
    "old content",
    "new content",
    Some("other-archive.txtar".to_string())
);
archive.add_edit(edit);
```

## Format Specification

### Basic Structure

```text
-- filename1.txt --
Content of file 1
Can span multiple lines

-- filename2.txt --
Content of file 2

-- subdir/file3.txt --
Content in subdirectory
```

### Binary Files

```text
-- binary.dat --
[.base64]
SGVsbG8gV29ybGQ=
```

### Edit References

```text
-- file.txt --
[edit:other.txt]
old content
-- new content --
```

## Documentation

- [API Documentation](https://docs.rs/emx-txtar)
- [Examples](https://github.com/coreseekdev/emx-txtar/tree/main/examples)

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Related Projects

- [emx-testspec](https://github.com/coreseekdev/emx-testspec) - E2E testing framework using txtar
- [Go txtar](https://pkg.go.dev/golang.org/x/tools/txtar) - Original Go implementation
