use sha2::{Sha256, Digest};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;
use anyhow::{Context, Result};
use crate::index::FileEntry;

/// Compute the SHA256 hash of a file
pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .context(format!("Failed to open file: {}", path.display()))?;
    
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 8192];
    
    loop {
        let bytes_read = file.read(&mut buffer)
            .context("Failed to read file")?;
        
        if bytes_read == 0 {
            break;
        }
        
        hasher.update(&buffer[..bytes_read]);
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}

/// Get the last modified time of a file in milliseconds since epoch
pub fn get_modified_time(path: &Path) -> Result<u64> {
    let metadata = fs::metadata(path)
        .context(format!("Failed to get metadata for: {}", path.display()))?;
    
    let modified = metadata.modified()
        .context("Failed to get modified time")?;
    
    let duration = modified.duration_since(SystemTime::UNIX_EPOCH)
        .context("Failed to compute duration since epoch")?;
    
    Ok(duration.as_millis() as u64)
}

/// Get the size of a file in bytes
pub fn get_file_size(path: &Path) -> Result<u64> {
    let metadata = fs::metadata(path)
        .context(format!("Failed to get metadata for: {}", path.display()))?;
    
    Ok(metadata.len())
}

/// Create a FileEntry from a file path
pub fn create_file_entry(path: &Path, relative_path: String) -> Result<FileEntry> {
    let num_bytes = get_file_size(path)?;
    let modified = get_modified_time(path)?;
    let sha256 = compute_sha256(path)?;
    
    Ok(FileEntry {
        num_bytes,
        modified,
        sha256,
        path: relative_path,
    })
}

/// Check if a file has changed based on size and modified time
pub fn has_changed(entry: &FileEntry, file_path: &Path) -> Result<bool> {
    let current_size = get_file_size(file_path)?;
    let current_modified = get_modified_time(file_path)?;
    
    Ok(current_size != entry.num_bytes || current_modified != entry.modified)
}

/// Format a FileEntry for display
pub fn format_entry(entry: &FileEntry) -> String {
    format!("{:>10} {:>15} {} {}", 
        entry.num_bytes,
        entry.modified,
        entry.sha256,
        entry.path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_sha256() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"hello world")?;
        temp_file.flush()?;
        
        let hash = compute_sha256(temp_file.path())?;
        // SHA256 of "hello world"
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
        
        Ok(())
    }

    #[test]
    fn test_get_file_size() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"hello")?;
        temp_file.flush()?;
        
        let size = get_file_size(temp_file.path())?;
        assert_eq!(size, 5);
        
        Ok(())
    }
}
