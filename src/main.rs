mod index;
mod commands;
mod file_utils;
mod ignore;
mod config;
mod scanner;
mod display;
mod dir_utils;

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
        
        /// Verbose mode - show all files including unchanged and ignored
        #[arg(short)]
        v: bool,
    },
    
    /// Update the index with changes from the filesystem
    Update {
        /// Pattern to update (file, directory, or glob pattern)
        pattern: Option<String>,
        
        /// Verbose mode - show all files including unchanged
        #[arg(short)]
        v: bool,
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
    
    /// Find duplicate files (files with identical content)
    Duplicates,
    
    /// Remove files that exist in another index
    Prune {
        /// Path to another oci index (source)
        source: Option<String>,
        
        /// Permanently delete pruned files
        #[arg(long)]
        purge: bool,
        
        /// Restore all pruned files
        #[arg(long)]
        restore: bool,
        
        /// Force operation without confirmation (for purge)
        #[arg(short, long)]
        force: bool,
        
        /// Don't prune files matching source's ignore patterns
        #[arg(long)]
        no_ignore: bool,
        
        /// Prune files matching local ignore patterns
        #[arg(long)]
        ignored: bool,
    },
    
    /// Reset the index (clear all entries)
    Reset {
        /// Force reset without confirmation
        #[arg(short)]
        f: bool,
    },
    
    /// Remove the index (opposite of init)
    Deinit {
        /// Force removal without confirmation
        #[arg(short)]
        f: bool,
    },
    
    /// Show index statistics
    Stats,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init(),
        Commands::Ignore { pattern } => commands::ignore(pattern),
        Commands::Status { path, r, v } => commands::status(path, r, v),
        Commands::Update { pattern, v } => commands::update(pattern, v),
        Commands::Ls { r } => commands::ls(r),
        Commands::Grep { hash } => commands::grep(&hash),
        Commands::Duplicates => commands::duplicates(),
        Commands::Prune { source, purge, restore, force, no_ignore, ignored } => commands::prune(source, purge, restore, force, no_ignore, ignored),
        Commands::Reset { f } => commands::reset(f),
        Commands::Deinit { f } => commands::deinit(f),
        Commands::Stats => commands::stats(),
    }
}
