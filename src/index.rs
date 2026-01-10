use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

pub const OCI_DIR: &str = ".oci";
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub num_bytes: u64,
    pub modified: u64,
    pub sha256: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Index {
    /// Map from file path (relative to repo root) to file entry
    pub files: HashMap<String, FileEntry>,
}

impl Index {
    /// Create a new empty index
    pub fn new() -> Self {
        Index {
            files: HashMap::new(),
        }
    }

    /// Load the index from disk
    pub fn load(repo_root: &Path) -> Result<Self> {
        let index_path = repo_root.join(OCI_DIR).join(INDEX_FILE);
        
        if !index_path.exists() {
            return Ok(Index::new());
        }

        let contents = fs::read_to_string(&index_path)
            .context("Failed to read index file")?;
        
        let index: Index = serde_json::from_str(&contents)
            .context("Failed to parse index file")?;
        
        Ok(index)
    }

    /// Save the index to disk
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let oci_dir = repo_root.join(OCI_DIR);
        fs::create_dir_all(&oci_dir)
            .context("Failed to create .oci directory")?;
        
        let index_path = oci_dir.join(INDEX_FILE);
        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize index")?;
        
        fs::write(&index_path, contents)
            .context("Failed to write index file")?;
        
        Ok(())
    }

    /// Add or update a file entry
    pub fn upsert(&mut self, entry: FileEntry) {
        self.files.insert(entry.path.clone(), entry);
    }

    /// Get a file entry
    pub fn get(&self, path: &str) -> Option<&FileEntry> {
        self.files.get(path)
    }

    /// Get all files in a directory (non-recursive)
    pub fn get_dir_files(&self, dir: &str) -> Vec<&FileEntry> {
        let normalized_dir = normalize_dir_path(dir);
        
        self.files.values()
            .filter(|entry| {
                let parent = Path::new(&entry.path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("");
                parent == normalized_dir
            })
            .collect()
    }

    /// Get all files in a directory (recursive)
    pub fn get_dir_files_recursive(&self, dir: &str) -> Vec<&FileEntry> {
        let normalized_dir = normalize_dir_path(dir);
        let prefix = if normalized_dir.is_empty() {
            String::new()
        } else {
            format!("{}/", normalized_dir)
        };

        self.files.values()
            .filter(|entry| {
                if prefix.is_empty() {
                    true
                } else {
                    entry.path.starts_with(&prefix)
                }
            })
            .collect()
    }

    /// Find all files with a given hash
    pub fn find_by_hash(&self, hash: &str) -> Vec<&FileEntry> {
        self.files.values()
            .filter(|entry| entry.sha256 == hash)
            .collect()
    }
}

/// Normalize a directory path for consistent comparison
fn normalize_dir_path(dir: &str) -> String {
    let trimmed = dir.trim_matches('/');
    if trimmed == "." {
        String::new()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_new() {
        let index = Index::new();
        assert_eq!(index.files.len(), 0);
    }

    #[test]
    fn test_index_upsert() {
        let mut index = Index::new();
        let entry = FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file.txt".to_string(),
        };
        
        index.upsert(entry.clone());
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.get("file.txt"), Some(&entry));
    }

    #[test]
    fn test_find_by_hash() {
        let mut index = Index::new();
        index.upsert(FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file1.txt".to_string(),
        });
        index.upsert(FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file2.txt".to_string(),
        });
        
        let results = index.find_by_hash("abc123");
        assert_eq!(results.len(), 2);
    }
}
