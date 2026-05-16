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

        Ok(ScanResult { tracked_files })
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_returns_all_files_when_no_patterns() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("a.txt"), "x").unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();
        fs::write(root.path().join("sub/b.txt"), "y").unwrap();

        let scanner = FileScanner::new(root.path().to_path_buf(), vec![]);
        let result = scanner.scan_repository_filtered(false).unwrap();

        assert!(result.tracked_files.contains("a.txt"));
        assert!(result.tracked_files.contains("sub/b.txt"));
    }

    #[test]
    fn test_scan_filters_by_pattern() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("keep.txt"), "x").unwrap();
        fs::write(root.path().join("drop.log"), "y").unwrap();

        let scanner = FileScanner::new(
            root.path().to_path_buf(),
            vec!["*.log".to_string()],
        );
        let result = scanner.scan_repository_filtered(false).unwrap();

        assert!(result.tracked_files.contains("keep.txt"));
        assert!(!result.tracked_files.contains("drop.log"));
    }

    #[test]
    fn test_scan_prunes_ignored_directories() {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join("node_modules")).unwrap();
        fs::write(root.path().join("node_modules/pkg.json"), "{}").unwrap();
        fs::write(root.path().join("app.js"), "x").unwrap();

        let scanner = FileScanner::new(
            root.path().to_path_buf(),
            vec!["node_modules/".to_string()],
        );
        let result = scanner.scan_repository_filtered(false).unwrap();

        assert!(result.tracked_files.contains("app.js"));
        assert!(result
            .tracked_files
            .iter()
            .all(|f| !f.starts_with("node_modules")));
    }
}
