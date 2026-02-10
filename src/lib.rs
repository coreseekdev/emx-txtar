//! # emx-txtar
//!
//! Txtar archive format implementation with binary file support.
//!
//! This crate extends the standard txtar format (used by Go's testscript) to support
//! binary files through base64 encoding.
//!
//! ## Standard Txtar Format
//!
//! A txtar archive is a text format that encodes multiple files:
//!
//! ```text
//! -- file1.txt --
//! content of file1
//! -- file2.txt --
//! content of file2
//! ```
//!
//! ## Binary Extension
//!
//! Binary files are encoded with a `[.base64]` suffix:
//!
//! ```text
//! -- image.jpg[.base64] --
//! /9j/4AAQSkZJRgABAQEAYABgAAD/2wBD...
//! ```
//!
//! ## Automatic Binary Detection
//!
//! Files are automatically marked as binary if:
//! - **Content conflict**: Content contains lines matching `-- xxxx --`
//! - **Invalid UTF-8**: Data is not valid UTF-8 encoded
//!
//! ## Encoding Detection (Extensible for i18n)
//!
//! The encoding detection is configurable via [`EncodingConfig`]:
//! - Enable/disable content marker checking
//! - Enable/disable UTF-8 validation
//! - Future: Support for UTF-16, GBK, ShiftJIS, etc.
//!
//! ## Binary Detection Rules
//!
//! Current detection rules (in order):
//! 1. Content has lines like `-- name --` → Binary (ContentConflict) **[PRIMARY]**
//! 2. Data is not valid UTF-8 → Binary (InvalidUtf8)
//! 3. Otherwise → Text (UTF-8)
//!
//! **Why content detection?**
//! The real issue is file CONTENT containing txtar marker patterns.
//! For example, a markdown file documenting txtar format would naturally
//! contain examples like `-- file.txt --`, which would corrupt the archive
//! structure if not encoded as binary.

pub mod archive;
pub mod encoder;
pub mod decoder;

pub use archive::{
    Archive, File,
    EncodingConfig, EncodingDetection, TextEncoding, BinaryReason,
    Command, SnippetRef, SnippetRefError, SnippetParseError,
    EditRef, EditBlock, EditOperation,
    EditParseError, EditApplyError,
};
pub use encoder::Encoder;
pub use decoder::Decoder;
