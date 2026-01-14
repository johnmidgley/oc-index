use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use walkdir::WalkDir;

use crate::ignore;

/// Result of scanning the filesystem
#[derive(Debug)]
pub struct ScanResult {
    /// Files that are not ignored
    pub tracked_files: HashSet<String>,
    /// Files that match ignore patterns (used in verbose mode)
    #[allow(dead_code)]
    pub ignored_files: HashSet<String>,
}

/// Utility for scanning directories with ignore pattern support
pub struct FileScanner {
    repo_root: PathBuf,
    patterns: Vec<String>,
}

impl FileScanner {
    /// Create a new FileScanner
    pub fn new(repo_root: PathBuf, patterns: Vec<String>) -> Self {
        Self {
            repo_root,
            patterns,
        }
    }


    /// Scan entire repository recursively with filtering
    pub fn scan_repository_filtered(&self, verbose: bool) -> Result<ScanResult> {
        let mut tracked_files = HashSet::new();
        let ignored_files = HashSet::new();

        for entry in WalkDir::new(&self.repo_root)
            .into_iter()
            .filter_entry(|e| {
                // Convert to relative path for pattern matching
                if let Ok(rel) = e.path().strip_prefix(&self.repo_root) {
                    !ignore::should_ignore(rel, &self.patterns)
                } else {
                    true // Don't filter if path conversion fails
                }
            })
        {
            // Handle permission errors gracefully - skip and continue
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    if verbose {
                        eprintln!("Warning: Skipping due to error: {}", err);
                    }
                    continue;
                }
            };

            if entry.file_type().is_file() {
                let rel_path = entry
                    .path()
                    .strip_prefix(&self.repo_root)
                    .context("Path is outside repository")?;
                tracked_files.insert(rel_path.to_string_lossy().to_string());
            }
        }

        Ok(ScanResult {
            tracked_files,
            ignored_files,
        })
    }

}
