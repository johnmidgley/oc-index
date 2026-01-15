use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const OCI_DIR: &str = ".oci";
const INDEX_FILE: &str = "index.db";

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub num_bytes: u64,
    pub modified: u64,
    pub sha256: String,
    pub path: String,
}

pub struct Index {
    conn: Connection,
    repo_root: Option<std::path::PathBuf>,
}

impl Index {
    /// Create a new empty index (in memory for testing)
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to create in-memory database")?;
        init_schema(&conn)?;
        Ok(Index { conn, repo_root: None })
    }

    /// Load the index from disk
    pub fn load(repo_root: &Path) -> Result<Self> {
        let oci_dir = repo_root.join(OCI_DIR);
        let index_path = oci_dir.join(INDEX_FILE);
        
        // Create directory if it doesn't exist
        std::fs::create_dir_all(&oci_dir)
            .context("Failed to create .oci directory")?;
        
        let conn = Connection::open(&index_path)
            .context("Failed to open index database")?;
        
        // Ensure schema exists (for new databases)
        init_schema(&conn)?;
        
        Ok(Index { 
            conn, 
            repo_root: Some(repo_root.to_path_buf()) 
        })
    }

    /// Save the index to disk (no-op for disk-based, required for in-memory)
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        // If this is a disk-based database (loaded from disk), it's already saved
        if self.repo_root.is_some() {
            return Ok(());
        }
        
        // For in-memory databases (e.g., tests or new index), backup to disk
        let oci_dir = repo_root.join(OCI_DIR);
        std::fs::create_dir_all(&oci_dir)
            .context("Failed to create .oci directory")?;
        
        let index_path = oci_dir.join(INDEX_FILE);
        
        let mut disk_conn = Connection::open(&index_path)
            .context("Failed to open destination database")?;
        
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut disk_conn)
            .context("Failed to create backup")?;
        
        backup.run_to_completion(5, std::time::Duration::from_millis(250), None)
            .context("Failed to backup database")?;
        
        Ok(())
    }

    /// Add or update a file entry
    pub fn upsert(&mut self, entry: FileEntry) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO files (path, num_bytes, modified, sha256) VALUES (?1, ?2, ?3, ?4)",
            params![entry.path, entry.num_bytes, entry.modified, entry.sha256],
        ).context("Failed to upsert file entry")?;
        Ok(())
    }

    /// Remove a file entry from the index
    pub fn remove(&mut self, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files WHERE path = ?1",
            params![path],
        ).context("Failed to remove file entry")?;
        Ok(())
    }

    /// Clear all entries from the index
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute("DELETE FROM files", [])
            .context("Failed to clear index")?;
        Ok(())
    }

    /// Get a file entry
    pub fn get(&self, path: &str) -> Result<Option<FileEntry>> {
        let result = self.conn.query_row(
            "SELECT path, num_bytes, modified, sha256 FROM files WHERE path = ?1",
            params![path],
            |row| {
                Ok(FileEntry {
                    path: row.get(0)?,
                    num_bytes: row.get(1)?,
                    modified: row.get(2)?,
                    sha256: row.get(3)?,
                })
            },
        ).optional().context("Failed to get file entry")?;
        
        Ok(result)
    }

    /// Get all files in a directory (non-recursive)
    pub fn get_dir_files(&self, dir: &str) -> Result<Vec<FileEntry>> {
        let normalized_dir = normalize_dir_path(dir);
        
        let mut stmt = self.conn.prepare(
            "SELECT path, num_bytes, modified, sha256 FROM files"
        ).context("Failed to prepare statement")?;
        
        let entries = stmt.query_map([], |row| {
            Ok(FileEntry {
                path: row.get(0)?,
                num_bytes: row.get(1)?,
                modified: row.get(2)?,
                sha256: row.get(3)?,
            })
        }).context("Failed to query files")?;
        
        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.context("Failed to read entry")?;
            let parent = Path::new(&entry.path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");
            
            if parent == normalized_dir {
                result.push(entry);
            }
        }
        
        Ok(result)
    }

    /// Get all files in a directory (recursive)
    pub fn get_dir_files_recursive(&self, dir: &str) -> Result<Vec<FileEntry>> {
        let normalized_dir = normalize_dir_path(dir);
        let prefix = if normalized_dir.is_empty() {
            String::new()
        } else {
            format!("{}/", normalized_dir)
        };

        let mut stmt = self.conn.prepare(
            "SELECT path, num_bytes, modified, sha256 FROM files"
        ).context("Failed to prepare statement")?;
        
        let entries = stmt.query_map([], |row| {
            Ok(FileEntry {
                path: row.get(0)?,
                num_bytes: row.get(1)?,
                modified: row.get(2)?,
                sha256: row.get(3)?,
            })
        }).context("Failed to query files")?;
        
        let mut result = Vec::new();
        for entry in entries {
            let file_entry: FileEntry = entry.context("Failed to read entry")?;
            // Filter by prefix
            if prefix.is_empty() || file_entry.path.starts_with(&prefix) {
                result.push(file_entry);
            }
        }
        
        Ok(result)
    }

    /// Find all files with a given hash
    pub fn find_by_hash(&self, hash: &str) -> Result<Vec<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, num_bytes, modified, sha256 FROM files WHERE sha256 = ?1"
        ).context("Failed to prepare statement")?;
        
        let entries = stmt.query_map(params![hash], |row| {
            Ok(FileEntry {
                path: row.get(0)?,
                num_bytes: row.get(1)?,
                modified: row.get(2)?,
                sha256: row.get(3)?,
            })
        }).context("Failed to query files by hash")?;
        
        let mut result = Vec::new();
        for entry in entries {
            result.push(entry.context("Failed to read entry")?);
        }
        
        Ok(result)
    }
}

/// Initialize the database schema
fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            path TEXT PRIMARY KEY,
            num_bytes INTEGER NOT NULL,
            modified INTEGER NOT NULL,
            sha256 TEXT NOT NULL
        )",
        [],
    ).context("Failed to create files table")?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sha256 ON files(sha256)",
        [],
    ).context("Failed to create sha256 index")?;
    
    Ok(())
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
        let index = Index::new().unwrap();
        let files = index.get_dir_files_recursive("").unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_index_upsert() {
        let mut index = Index::new().unwrap();
        let entry = FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file.txt".to_string(),
        };
        
        index.upsert(entry.clone()).unwrap();
        let files = index.get_dir_files_recursive("").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(index.get("file.txt").unwrap(), Some(entry));
    }

    #[test]
    fn test_find_by_hash() {
        let mut index = Index::new().unwrap();
        index.upsert(FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file1.txt".to_string(),
        }).unwrap();
        index.upsert(FileEntry {
            num_bytes: 100,
            modified: 1000,
            sha256: "abc123".to_string(),
            path: "file2.txt".to_string(),
        }).unwrap();
        
        let results = index.find_by_hash("abc123").unwrap();
        assert_eq!(results.len(), 2);
    }
}
