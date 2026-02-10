//! emx-txtar CLI
//!
//! Create and extract txtar archives (similar to tar command).

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use emx_txtar::{Archive, File, Encoder, Decoder};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "emx-txtar")]
#[command(author = "nzinfo <li.monan@gmail.com>")]
#[command(version)]
#[command(about = "Txtar archive format tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a txtar archive from files/directories
    Create {
        /// Files and directories to archive
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Output archive file (default: stdout)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Extract a txtar archive
    #[command(name = "x")]
    Extract {
        /// Archive file to extract (default: stdin)
        #[arg(short = 'i', long)]
        input: Option<PathBuf>,

        /// Directory to extract to (default: current directory)
        #[arg(short = 'C', long, default_value = ".")]
        directory: PathBuf,

        /// Include snippet files
        #[arg(long)]
        include_snippets: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// List contents of a txtar archive
    #[command(name = "t")]
    List {
        /// Archive file to list (default: stdin)
        #[arg(short = 'i', long)]
        input: Option<PathBuf>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create { inputs, output, verbose } => {
            create_archive(inputs, output, verbose)?;
        }
        Commands::Extract { input, directory, include_snippets, verbose } => {
            extract_archive(input, directory, include_snippets, verbose)?;
        }
        Commands::List { input, verbose } => {
            list_archive(input, verbose)?;
        }
    }

    Ok(())
}

fn create_archive(inputs: Vec<PathBuf>, output: Option<PathBuf>, verbose: bool) -> Result<()> {
    let mut archive = Archive::new();

    for input in &inputs {
        if input.is_dir() {
            add_directory(&mut archive, input, verbose)?;
        } else {
            let content = fs::read(input)
                .with_context(|| format!("Failed to read file: {}", input.display()))?;

            let name = input.file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?
                .to_string_lossy()
                .to_string();

            archive.add_file(File::new(&name, content.clone()));

            if verbose {
                println!("Added: {} ({} bytes)", name, content.len());
            }
        }
    }

    let encoder = Encoder::new();
    let txtar_content = encoder.encode(&archive)?;

    if let Some(output_path) = output {
        fs::write(&output_path, txtar_content)
            .with_context(|| format!("Failed to write: {}", output_path.display()))?;

        if verbose {
            println!("Created: {} ({} files)", output_path.display(), archive.files.len());
        }
    } else {
        print!("{}", txtar_content);
    }

    Ok(())
}

fn add_directory(archive: &mut Archive, dir: &Path, verbose: bool) -> Result<()> {
    #[cfg(feature = "walkdir")]
    {
        let entries = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect::<Vec<_>>();

        for entry in entries {
            let path = entry.path();
            let content = fs::read(&path)
                .with_context(|| format!("Failed to read: {}", path.display()))?;

            let relative_path = path.strip_prefix(dir)
                .map_err(|_| anyhow::anyhow!("Failed to get relative path"))?;

            let name = relative_path.to_string_lossy().replace('\\', "/");
            archive.add_file(File::new(&name, content.clone()));

            if verbose {
                println!("Added: {} ({} bytes)", name, content.len());
            }
        }
    }

    #[cfg(not(feature = "walkdir"))]
    {
        anyhow::bail!("Directory traversal requires the 'cli' feature");
    }

    Ok(())
}

fn extract_archive(
    input: Option<PathBuf>,
    directory: PathBuf,
    include_snippets: bool,
    verbose: bool,
) -> Result<()> {
    let txtar_content = if let Some(input_path) = input {
        fs::read_to_string(&input_path)
            .with_context(|| format!("Failed to read: {}", input_path.display()))?
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };

    let decoder = Decoder::new();
    let archive = decoder.decode(&txtar_content)?;

    if verbose {
        println!("Files: {}", archive.files.len());
    }

    for file in &archive.files {
        if file.snippet_ref.is_some() && !include_snippets {
            if verbose {
                println!("Skipped snippet: {}", file.name);
            }
            continue;
        }

        let output_path = directory.join(&file.name);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&output_path, &file.data)?;

        if verbose {
            println!("Extracted: {}", file.name);
        }
    }

    Ok(())
}

fn list_archive(input: Option<PathBuf>, verbose: bool) -> Result<()> {
    let txtar_content = if let Some(input_path) = input {
        fs::read_to_string(&input_path)?
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };

    let decoder = Decoder::new();
    let archive = decoder.decode(&txtar_content)?;

    for file in &archive.files {
        if verbose {
            let enc = if file.is_binary { "binary" } else { "text" };
            println!("{}  {}  {}", file.name, enc, file.data.len());
        } else {
            println!("{}", file.name);
        }
    }

    Ok(())
}
