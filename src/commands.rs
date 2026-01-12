use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::file_utils;
use crate::ignore;
use crate::index::{Index, OCI_DIR};

/// Find the repository root by looking for .oci directory
fn find_repo_root() -> Result<PathBuf> {
    let mut current_dir = env::current_dir()
        .context("Failed to get current directory")?;
    
    loop {
        let oci_path = current_dir.join(OCI_DIR);
        if oci_path.exists() && oci_path.is_dir() {
            return Ok(current_dir);
        }
        
        if !current_dir.pop() {
            bail!("Not in an oci repository (or any parent directory)");
        }
    }
}

/// Initialize a new index
pub fn init() -> Result<()> {
    let current_dir = env::current_dir()
        .context("Failed to get current directory")?;
    
    let oci_dir = current_dir.join(OCI_DIR);
    
    if oci_dir.exists() {
        bail!("Index already exists at {}", oci_dir.display());
    }
    
    fs::create_dir_all(&oci_dir)
        .context("Failed to create .oci directory")?;
    
    let index = Index::new()?;
    index.save(&current_dir)?;
    
    println!("Initialized empty oci index in {}", oci_dir.display());
    Ok(())
}

/// Add a pattern to the ignore list
pub fn ignore(pattern: Option<String>) -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    
    let pattern_to_add = if let Some(p) = pattern {
        // Convert relative path to absolute from repo root
        if Path::new(&p).is_relative() {
            let full_path = current_dir.join(&p);
            let rel_path = full_path.strip_prefix(&repo_root)
                .context("Path is outside repository")?;
            rel_path.to_string_lossy().to_string()
        } else {
            p
        }
    } else {
        // Use current directory
        let rel_path = current_dir.strip_prefix(&repo_root)
            .context("Current directory is outside repository")?;
        rel_path.to_string_lossy().to_string()
    };
    
    ignore::add_pattern(&repo_root, &pattern_to_add)?;
    println!("Added pattern to .ocignore: {}", pattern_to_add);
    
    Ok(())
}

/// Check status of files
pub fn status(recursive: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;
    let patterns = ignore::load_patterns(&repo_root)?;
    
    let rel_current = current_dir.strip_prefix(&repo_root)
        .context("Current directory is outside repository")?;
    let rel_current_str = rel_current.to_string_lossy().to_string();
    
    // Get all files from filesystem
    let mut fs_files = std::collections::HashSet::new();
    
    let walker = if recursive {
        WalkDir::new(&current_dir).into_iter()
    } else {
        WalkDir::new(&current_dir).max_depth(1).into_iter()
    };
    
    for entry in walker.filter_entry(|e| !ignore::should_ignore(e.path(), &patterns)) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let rel_path = entry.path().strip_prefix(&repo_root)
                .context("Path is outside repository")?;
            fs_files.insert(rel_path.to_string_lossy().to_string());
        }
    }
    
    // Get indexed files for comparison
    let indexed_files: Vec<_> = if recursive {
        index.get_dir_files_recursive(&rel_current_str)?
    } else {
        index.get_dir_files(&rel_current_str)?
    };
    
    let mut has_changes = false;
    
    // Check for modified and added files
    for fs_path in &fs_files {
        let full_path = repo_root.join(fs_path);
        
        if let Some(entry) = index.get(fs_path)? {
            // File exists in index - check if modified
            if file_utils::has_changed(&entry, &full_path)? {
                // Display relative to current directory
                let display_path = make_relative_to_current(&repo_root, &current_dir, fs_path)?;
                println!("M {}", file_utils::format_entry(&create_entry_with_path(&full_path, display_path)?));
                has_changes = true;
            }
        } else {
            // File not in index - added
            let display_path = make_relative_to_current(&repo_root, &current_dir, fs_path)?;
            println!("+ {}", file_utils::format_entry(&create_entry_with_path(&full_path, display_path)?));
            has_changes = true;
        }
    }
    
    // Check for deleted files
    for entry in indexed_files {
        if !fs_files.contains(&entry.path) {
            let display_path = make_relative_to_current(&repo_root, &current_dir, &entry.path)?;
            let mut display_entry = entry.clone();
            display_entry.path = display_path;
            println!("- {}", file_utils::format_entry(&display_entry));
            has_changes = true;
        }
    }
    
    if !has_changes {
        println!("No changes");
    }
    
    Ok(())
}

/// Update the index with changes from the filesystem
pub fn update(pattern: Option<String>) -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    let mut index = Index::load(&repo_root)?;
    let patterns = ignore::load_patterns(&repo_root)?;
    
    let target_path = if let Some(p) = pattern {
        current_dir.join(p)
    } else {
        repo_root.clone()
    };
    
    if !target_path.exists() {
        bail!("Path does not exist: {}", target_path.display());
    }
    
    let mut updated_count = 0;
    let mut skipped_count = 0;
    
    if target_path.is_file() {
        // Commit single file
        let rel_path = target_path.strip_prefix(&repo_root)
            .context("Path is outside repository")?;
        let rel_path_str = rel_path.to_string_lossy().to_string();
        
        if !ignore::should_ignore(&target_path, &patterns) {
            if should_update_file(&index, &target_path, &rel_path_str)? {
                let entry = file_utils::create_file_entry(&target_path, rel_path_str)?;
                index.upsert(entry)?;
                updated_count += 1;
            } else {
                skipped_count += 1;
            }
        }
    } else {
        // Commit directory recursively
        for entry in WalkDir::new(&target_path).into_iter()
            .filter_entry(|e| !ignore::should_ignore(e.path(), &patterns)) {
            let entry = entry?;
            
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&repo_root)
                    .context("Path is outside repository")?;
                let rel_path_str = rel_path.to_string_lossy().to_string();
                
                if should_update_file(&index, entry.path(), &rel_path_str)? {
                    let file_entry = file_utils::create_file_entry(entry.path(), rel_path_str)?;
                    index.upsert(file_entry)?;
                    updated_count += 1;
                } else {
                    skipped_count += 1;
                }
            }
        }
    }
    
    index.save(&repo_root)?;
    println!("Updated {} file(s) in the index", updated_count);
    if skipped_count > 0 {
        println!("Skipped {} unchanged file(s)", skipped_count);
    }
    
    Ok(())
}

/// List files in the index
pub fn ls(recursive: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;
    
    let rel_current = current_dir.strip_prefix(&repo_root)
        .context("Current directory is outside repository")?;
    let rel_current_str = rel_current.to_string_lossy().to_string();
    
    let mut entries: Vec<_> = if recursive {
        index.get_dir_files_recursive(&rel_current_str)?
    } else {
        index.get_dir_files(&rel_current_str)?
    };
    
    if entries.is_empty() {
        println!("No files in index");
        return Ok(());
    }
    
    // Sort by path for consistent output
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    
    for entry in entries {
        let display_path = make_relative_to_current(&repo_root, &current_dir, &entry.path)?;
        let mut display_entry = entry.clone();
        display_entry.path = display_path;
        println!("{}", file_utils::format_entry(&display_entry));
    }
    
    Ok(())
}

/// Find files by hash
pub fn grep(hash: &str) -> Result<()> {
    let repo_root = find_repo_root()?;
    let index = Index::load(&repo_root)?;
    
    let matches = index.find_by_hash(hash)?;
    
    if matches.is_empty() {
        println!("No files found with hash: {}", hash);
        return Ok(());
    }
    
    println!("Found {} file(s) with hash {}:", matches.len(), hash);
    for entry in matches {
        println!("{}", file_utils::format_entry(&entry));
    }
    
    Ok(())
}

/// Remove the index
pub fn rm(force: bool) -> Result<()> {
    if !force {
        bail!("The -f flag is required to remove the index");
    }
    
    let repo_root = find_repo_root()?;
    let oci_dir = repo_root.join(OCI_DIR);
    
    fs::remove_dir_all(&oci_dir)
        .context("Failed to remove .oci directory")?;
    
    println!("Removed index at {}", oci_dir.display());
    Ok(())
}

/// Check if a file should be updated in the index
/// Returns true if the file is new or has changed (size or modified time differ)
fn should_update_file(index: &Index, file_path: &Path, rel_path: &str) -> Result<bool> {
    if let Some(entry) = index.get(rel_path)? {
        // File exists in index - check if it has changed
        file_utils::has_changed(&entry, file_path)
    } else {
        // File not in index - needs to be added
        Ok(true)
    }
}

/// Helper function to create a file entry with a specific display path
fn create_entry_with_path(full_path: &Path, display_path: String) -> Result<crate::index::FileEntry> {
    let num_bytes = file_utils::get_file_size(full_path)?;
    let modified = file_utils::get_modified_time(full_path)?;
    let sha256 = file_utils::compute_sha256(full_path)?;
    
    Ok(crate::index::FileEntry {
        num_bytes,
        modified,
        sha256,
        path: display_path,
    })
}

/// Make a path relative to the current directory for display
fn make_relative_to_current(repo_root: &Path, current_dir: &Path, file_path: &str) -> Result<String> {
    let full_file_path = repo_root.join(file_path);
    
    if let Ok(rel) = full_file_path.strip_prefix(current_dir) {
        Ok(rel.to_string_lossy().to_string())
    } else {
        // File is outside current directory, show full path from repo root
        Ok(file_path.to_string())
    }
}
