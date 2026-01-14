use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Remove empty parent directories recursively up to the repo root
pub fn remove_empty_parent_dirs(file_path: &Path, repo_root: &Path) -> Result<()> {
    if let Some(mut parent) = file_path.parent() {
        // Walk up the directory tree
        while parent != repo_root && parent.starts_with(repo_root) {
            // Try to read the directory
            match fs::read_dir(parent) {
                Ok(mut entries) => {
                    // Check if directory is empty
                    if entries.next().is_none() {
                        // Directory is empty, remove it
                        fs::remove_dir(parent).context(format!(
                            "Failed to remove empty directory: {}",
                            parent.display()
                        ))?;
                        // Move up to parent
                        if let Some(p) = parent.parent() {
                            parent = p;
                        } else {
                            break;
                        }
                    } else {
                        // Directory is not empty, stop
                        break;
                    }
                }
                Err(_) => {
                    // Can't read directory, stop
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Remove all empty directories in the repository (recursive pass)
pub fn remove_all_empty_dirs(repo_root: &Path) -> Result<usize> {
    let mut removed_count = 0;
    let mut found_empty = true;

    // Keep scanning until no more empty directories are found
    // (needed because removing a directory might make its parent empty)
    while found_empty {
        found_empty = false;
        let mut dirs_to_remove = Vec::new();

        // Collect all directories (walk depth-first, post-order)
        // We need to collect them first, then remove, to avoid iterator invalidation
        for entry in WalkDir::new(repo_root).min_depth(1).contents_first(true) {
            let entry = entry?;

            // Skip .oci directory
            if let Ok(rel) = entry.path().strip_prefix(repo_root) {
                let rel_str = rel.to_string_lossy();
                if rel_str.starts_with(".oci") {
                    continue;
                }
            }

            if entry.file_type().is_dir() {
                // Check if directory is empty
                if let Ok(mut entries) = fs::read_dir(entry.path()) {
                    if entries.next().is_none() {
                        dirs_to_remove.push(entry.path().to_path_buf());
                    }
                }
            }
        }

        // Remove all empty directories found in this pass
        for dir in dirs_to_remove {
            if fs::remove_dir(&dir).is_ok() {
                removed_count += 1;
                found_empty = true;
            }
        }
    }

    Ok(removed_count)
}

/// Count files in a directory recursively
pub fn count_files_in_dir(dir: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            count += 1;
        }
    }

    Ok(count)
}
