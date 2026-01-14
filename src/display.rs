use anyhow::Result;
use std::path::Path;

use crate::file_utils;
use crate::index::FileEntry;

/// Helper to compute display paths relative to current directory
pub struct DisplayContext {
    repo_root: std::path::PathBuf,
    current_dir: std::path::PathBuf,
}

impl DisplayContext {
    /// Create a new DisplayContext
    pub fn new(repo_root: std::path::PathBuf, current_dir: std::path::PathBuf) -> Self {
        Self {
            repo_root,
            current_dir,
        }
    }

    /// Make a path relative to the current directory for display
    pub fn make_relative(&self, file_path: &str) -> Result<String> {
        let full_file_path = self.repo_root.join(file_path);

        if let Ok(rel) = full_file_path.strip_prefix(&self.current_dir) {
            Ok(rel.to_string_lossy().to_string())
        } else {
            // File is outside current directory, show full path from repo root
            Ok(file_path.to_string())
        }
    }

    /// Create a FileEntry with a display path for output (computes hash)
    #[allow(dead_code)]
    pub fn create_display_entry(&self, full_path: &Path, display_path: String) -> Result<FileEntry> {
        let num_bytes = file_utils::get_file_size(full_path)?;
        let modified = file_utils::get_modified_time(full_path)?;
        let sha256 = file_utils::compute_sha256(full_path)?;

        Ok(FileEntry {
            num_bytes,
            modified,
            sha256,
            path: display_path,
        })
    }

    /// Create a FileEntry for status display (without computing hash)
    pub fn create_status_entry(&self, full_path: &Path, display_path: String) -> Result<FileEntry> {
        let num_bytes = file_utils::get_file_size(full_path)?;
        let modified = file_utils::get_modified_time(full_path)?;

        Ok(FileEntry {
            num_bytes,
            modified,
            sha256: String::new(), // Empty hash for status display
            path: display_path,
        })
    }

    /// Format an entry with a relative path for display
    pub fn format_entry_relative(&self, entry: &FileEntry) -> Result<String> {
        let display_path = self.make_relative(&entry.path)?;
        let mut display_entry = entry.clone();
        display_entry.path = display_path;
        Ok(file_utils::format_entry(&display_entry))
    }
}

/// Status markers for file changes
pub enum StatusMarker {
    Added,
    Updated,
    Deleted,
    Unchanged,
    Ignored,
}

impl StatusMarker {
    pub fn symbol(&self) -> &'static str {
        match self {
            StatusMarker::Added => "+",
            StatusMarker::Updated => "U",
            StatusMarker::Deleted => "-",
            StatusMarker::Unchanged => "=",
            StatusMarker::Ignored => "I",
        }
    }

    pub fn display(&self, formatted_entry: &str) {
        println!("{} {}", self.symbol(), formatted_entry);
    }
}
