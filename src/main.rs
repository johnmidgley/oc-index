mod index;
mod commands;
mod file_utils;
mod ignore;

use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "oci")]
#[command(about = "A command line tool that creates an index of files by hash", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize an empty index
    Init,
    
    /// Add patterns to the ignore list
    Ignore {
        /// Pattern to ignore (file, directory, or glob pattern)
        pattern: Option<String>,
    },
    
    /// Check for differences between the index and filesystem
    Status {
        /// Path to check (file or directory)
        path: Option<String>,
        
        /// Recurse into subdirectories
        #[arg(short)]
        r: bool,
    },
    
    /// Update the index with changes from the filesystem
    Update {
        /// Pattern to update (file, directory, or glob pattern)
        pattern: Option<String>,
    },
    
    /// List files in the index
    Ls {
        /// Recurse into subdirectories
        #[arg(short)]
        r: bool,
    },
    
    /// Find files by hash
    Grep {
        /// SHA256 hash to search for
        hash: String,
    },
    
    /// Remove the index
    Rm {
        /// Force removal (required)
        #[arg(short)]
        f: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init(),
        Commands::Ignore { pattern } => commands::ignore(pattern),
        Commands::Status { path, r } => commands::status(path, r),
        Commands::Update { pattern } => commands::update(pattern),
        Commands::Ls { r } => commands::ls(r),
        Commands::Grep { hash } => commands::grep(&hash),
        Commands::Rm { f } => commands::rm(f),
    }
}
