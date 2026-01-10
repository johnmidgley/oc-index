use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "oci")]
#[command(about = "A command line tool that creates an index of files in a directory")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize oci in the current directory
    Init,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            if let Err(e) = init_index() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug)]
struct FileEntry {
    num_bytes: u64,
    modified: u128,
    sha256: String,
    name: String,
    dir: PathBuf,
}

fn init_index() -> Result<(), Box<dyn std::error::Error>> {
    // Get current directory
    let current_dir = std::env::current_dir()?;
    let oci_dir = current_dir.join(".oci");
    
    // Check if .oci directory already exists
    if oci_dir.exists() {
        eprintln!("Error: .oci directory already exists. Cannot initialize.");
        std::process::exit(1);
    }
    
    // Create .oci directory
    fs::create_dir(&oci_dir)?;
    
    // Collect all directories and files
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut file_entries: Vec<FileEntry> = Vec::new();
    
    // Recursively traverse all directories
    let walker = WalkDir::new(&current_dir)
        .into_iter()
        .filter_entry(|e| {
            // Skip the .oci directory itself
            !e.path().starts_with(&oci_dir)
        });
    
    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        
        // Skip .oci directory entries
        if path.starts_with(&oci_dir) {
            continue;
        }
        
        if entry.file_type().is_dir() {
            let rel_path = path.strip_prefix(&current_dir)?.to_path_buf();
            // Include root directory (.) and all subdirectories
            directories.push(rel_path);
        } else if entry.file_type().is_file() {
            // Get file metadata
            let metadata = fs::metadata(path)?;
            let num_bytes = metadata.len();
            
            // Get modified time as epoch milliseconds
            let modified = metadata.modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis();
            
            // Calculate SHA256 hash
            let mut file = fs::File::open(path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let mut hasher = Sha256::new();
            hasher.update(&buffer);
            let hash = hasher.finalize();
            let sha256 = format!("{:x}", hash);
            
            // Get relative paths
            let rel_path = path.strip_prefix(&current_dir)?;
            let dir = rel_path.parent().unwrap_or(Path::new(".")).to_path_buf();
            // Get basename (filename only, not full path)
            let name = path.file_name()
                .ok_or_else(|| format!("Failed to get filename for path: {:?}", path))?
                .to_string_lossy()
                .to_string();
            
            file_entries.push(FileEntry {
                num_bytes,
                modified,
                sha256,
                name,
                dir,
            });
        }
    }
    
    // Sort directories for consistent output
    directories.sort();
    
    // Create index.txt file
    let index_path = oci_dir.join("index.txt");
    let mut index_file = fs::File::create(&index_path)?;
    
    // Output each directory path followed by its files
    for dir in directories {
        // Write directory path (use "." for root, otherwise relative path)
        let dir_str = if dir == Path::new(".") {
            ".".to_string()
        } else {
            dir.to_string_lossy().to_string()
        };
        writeln!(index_file, "{}", dir_str)?;
        
        // Write all files in this directory
        let files_in_dir: Vec<_> = file_entries
            .iter()
            .filter(|e| e.dir == dir)
            .collect();
        
        for file_entry in files_in_dir {
            writeln!(
                index_file,
                "{} {} {} {}",
                file_entry.num_bytes,
                file_entry.modified,
                file_entry.sha256,
                file_entry.name
            )?;
        }
    }
    
    Ok(())
}
