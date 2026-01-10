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
    /// Remove the oci index from the current directory
    Rm,
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
        Commands::Rm => {
            if let Err(e) = rm_index() {
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

fn ensure_oci_dir(oci_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if oci_dir.exists() {
        eprintln!("Error: .oci directory already exists. Cannot initialize.");
        std::process::exit(1);
    }
    fs::create_dir(oci_dir)?;
    Ok(())
}

fn calculate_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

fn create_file_entry(
    path: &Path,
    current_dir: &Path,
) -> Result<FileEntry, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    let num_bytes = metadata.len();
    
    let modified = metadata.modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    
    let sha256 = calculate_sha256(path)?;
    
    let rel_path = path.strip_prefix(current_dir)?;
    let dir = rel_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    
    let name = path.file_name()
        .ok_or_else(|| format!("Failed to get filename for path: {:?}", path))?
        .to_string_lossy()
        .to_string();
    
    Ok(FileEntry {
        num_bytes,
        modified,
        sha256,
        name,
        dir,
    })
}

fn scan_directory(current_dir: &Path, oci_dir: &Path) -> Result<(Vec<PathBuf>, Vec<FileEntry>), Box<dyn std::error::Error>> {
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut file_entries: Vec<FileEntry> = Vec::new();
    
    let walker = WalkDir::new(current_dir)
        .into_iter()
        .filter_entry(|e| !e.path().starts_with(oci_dir));
    
    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        
        if path.starts_with(oci_dir) {
            continue;
        }
        
        if entry.file_type().is_dir() {
            let rel_path = path.strip_prefix(current_dir)?.to_path_buf();
            directories.push(rel_path);
        } else if entry.file_type().is_file() {
            let file_entry = create_file_entry(path, current_dir)?;
            file_entries.push(file_entry);
        }
    }
    
    directories.sort();
    Ok((directories, file_entries))
}

fn write_index_file(
    oci_dir: &Path,
    directories: Vec<PathBuf>,
    file_entries: Vec<FileEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let index_path = oci_dir.join("index.txt");
    let mut index_file = fs::File::create(&index_path)?;
    
    for dir in directories {
        let dir_str = if dir == Path::new(".") {
            ".".to_string()
        } else {
            dir.to_string_lossy().to_string()
        };
        writeln!(index_file, "{}", dir_str)?;
        
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

fn init_index() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    let oci_dir = current_dir.join(".oci");
    
    ensure_oci_dir(&oci_dir)?;
    let (directories, file_entries) = scan_directory(&current_dir, &oci_dir)?;
    write_index_file(&oci_dir, directories, file_entries)?;
    
    Ok(())
}

fn rm_index() -> Result<(), Box<dyn std::error::Error>> {
    // Get current directory
    let current_dir = std::env::current_dir()?;
    let oci_dir = current_dir.join(".oci");
    
    // Check if .oci directory exists
    if !oci_dir.exists() {
        eprintln!("Error: .oci directory does not exist. No index to remove.");
        std::process::exit(1);
    }
    
    // Remove the .oci directory and all its contents
    fs::remove_dir_all(&oci_dir)?;
    
    Ok(())
}
