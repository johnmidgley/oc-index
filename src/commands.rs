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
    
    // Initialize .ocignore with default patterns
    ignore::init_ignore_file(&current_dir)?;
    
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
pub fn status(pattern: Option<String>, recursive: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;
    let patterns = ignore::load_patterns(&repo_root)?;
    
    // Determine what to scan based on arguments
    let (scan_dir, scan_rel_path, is_recursive) = if let Some(p) = pattern {
        // Path argument provided
        let target_path = current_dir.join(&p);
        if !target_path.exists() {
            bail!("Path does not exist: {}", target_path.display());
        }
        
        let rel_path = target_path.strip_prefix(&repo_root)
            .context("Path is outside repository")?;
        let rel_path_str = rel_path.to_string_lossy().to_string();
        
        // If it's a file, always non-recursive; if directory, use recursive flag
        let is_recursive = target_path.is_dir() && recursive;
        (target_path, rel_path_str, is_recursive)
    } else if recursive {
        // No path, but -r flag: scan from current directory recursively
        let rel_current = current_dir.strip_prefix(&repo_root)
            .context("Current directory is outside repository")?;
        (current_dir.clone(), rel_current.to_string_lossy().to_string(), true)
    } else {
        // No path, no -r flag: scan entire repository from root
        (repo_root.clone(), String::new(), true)
    };
    
    // Get all files from filesystem
    let mut fs_files = std::collections::HashSet::new();
    
    if scan_dir.is_file() {
        // Single file
        let rel_path = scan_dir.strip_prefix(&repo_root)
            .context("Path is outside repository")?;
        fs_files.insert(rel_path.to_string_lossy().to_string());
    } else {
        // Directory
        let walker = if is_recursive {
            WalkDir::new(&scan_dir).into_iter()
        } else {
            WalkDir::new(&scan_dir).max_depth(1).into_iter()
        };
        
        for entry in walker.filter_entry(|e| {
            // Convert to relative path for pattern matching
            if let Ok(rel) = e.path().strip_prefix(&repo_root) {
                !ignore::should_ignore(rel, &patterns)
            } else {
                true // Don't filter if path conversion fails
            }
        }) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&repo_root)
                    .context("Path is outside repository")?;
                fs_files.insert(rel_path.to_string_lossy().to_string());
            }
        }
    }
    
    // Get indexed files for comparison
    let indexed_files: Vec<_> = if is_recursive {
        index.get_dir_files_recursive(&scan_rel_path)?
    } else {
        index.get_dir_files(&scan_rel_path)?
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
                println!("U {}", file_utils::format_entry(&create_entry_with_path(&full_path, display_path)?));
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
    
    let mut added_count = 0;
    let mut updated_count = 0;
    let mut removed_count = 0;
    let mut skipped_count = 0;
    
    if target_path.is_file() {
        // Update single file
        let rel_path = target_path.strip_prefix(&repo_root)
            .context("Path is outside repository")?;
        let rel_path_str = rel_path.to_string_lossy().to_string();
        
        if !ignore::should_ignore(rel_path, &patterns) {
            let is_new = index.get(&rel_path_str)?.is_none();
            
            if should_update_file(&index, &target_path, &rel_path_str)? {
                let display_path = make_relative_to_current(&repo_root, &current_dir, &rel_path_str)?;
                let prefix = if is_new { "+" } else { "U" };
                println!("{} {}", prefix, display_path);
                
                let entry = file_utils::create_file_entry(&target_path, rel_path_str)?;
                index.upsert(entry)?;
                
                if is_new {
                    added_count += 1;
                } else {
                    updated_count += 1;
                }
            } else {
                skipped_count += 1;
            }
        }
    } else {
        // Update directory recursively
        // First, collect all files that exist on disk
        let mut fs_files = std::collections::HashSet::new();
        
        for entry in WalkDir::new(&target_path).into_iter()
            .filter_entry(|e| {
                // Convert to relative path for pattern matching
                if let Ok(rel) = e.path().strip_prefix(&repo_root) {
                    !ignore::should_ignore(rel, &patterns)
                } else {
                    true // Don't filter if path conversion fails
                }
            }) {
            let entry = entry?;
            
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&repo_root)
                    .context("Path is outside repository")?;
                let rel_path_str = rel_path.to_string_lossy().to_string();
                fs_files.insert(rel_path_str.clone());
                
                let is_new = index.get(&rel_path_str)?.is_none();
                
                if should_update_file(&index, entry.path(), &rel_path_str)? {
                    let display_path = make_relative_to_current(&repo_root, &current_dir, &rel_path_str)?;
                    let prefix = if is_new { "+" } else { "U" };
                    println!("{} {}", prefix, display_path);
                    
                    let file_entry = file_utils::create_file_entry(entry.path(), rel_path_str)?;
                    index.upsert(file_entry)?;
                    
                    if is_new {
                        added_count += 1;
                    } else {
                        updated_count += 1;
                    }
                } else {
                    skipped_count += 1;
                }
            }
        }
        
        // Now check for deleted files in the index
        let rel_target = target_path.strip_prefix(&repo_root)
            .context("Path is outside repository")?;
        let rel_target_str = rel_target.to_string_lossy().to_string();
        
        let indexed_files = index.get_dir_files_recursive(&rel_target_str)?;
        
        for indexed_entry in indexed_files {
            if !fs_files.contains(&indexed_entry.path) {
                // File is in index but not on disk - remove it
                let display_path = make_relative_to_current(&repo_root, &current_dir, &indexed_entry.path)?;
                println!("- {}", display_path);
                index.remove(&indexed_entry.path)?;
                removed_count += 1;
            }
        }
    }
    
    index.save(&repo_root)?;
    
    let total_changed = added_count + updated_count + removed_count;
    if total_changed > 0 {
        println!("Updated {} file(s) in the index ({} added, {} updated, {} removed)", 
                 total_changed, added_count, updated_count, removed_count);
    } else {
        println!("Updated 0 file(s) in the index");
    }
    
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

/// Find duplicate files (files with identical content)
pub fn duplicates() -> Result<()> {
    let repo_root = find_repo_root()?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;
    
    // Get all files from the repository recursively
    let entries: Vec<_> = index.get_dir_files_recursive("")?;
    
    // Group files by hash
    let mut hash_groups: std::collections::HashMap<String, Vec<crate::index::FileEntry>> = 
        std::collections::HashMap::new();
    
    for entry in entries {
        hash_groups.entry(entry.sha256.clone())
            .or_default()
            .push(entry);
    }
    
    // Filter to only hashes with duplicates (more than 1 file)
    let mut duplicate_groups: Vec<_> = hash_groups.into_iter()
        .filter(|(_, files)| files.len() > 1)
        .collect();
    
    if duplicate_groups.is_empty() {
        println!("No duplicate files found");
        return Ok(());
    }
    
    // Sort groups by hash for consistent output
    duplicate_groups.sort_by(|a, b| a.0.cmp(&b.0));
    
    // Calculate statistics
    let total_duplicate_files: usize = duplicate_groups.iter()
        .map(|(_, files)| files.len())
        .sum();
    let total_groups = duplicate_groups.len();
    
    // Calculate wasted space (all but one copy of each duplicate set)
    let wasted_bytes: u64 = duplicate_groups.iter()
        .map(|(_, files)| {
            let file_size = files[0].num_bytes;
            file_size * (files.len() as u64 - 1)
        })
        .sum();
    
    println!("Found {} duplicate file(s) in {} group(s)", total_duplicate_files, total_groups);
    println!("Potential space savings: {} bytes ({:.2} MB)\n", 
             wasted_bytes, 
             wasted_bytes as f64 / 1_048_576.0);
    
    // Display each group
    for (hash, mut files) in duplicate_groups {
        println!("Hash: {}", hash);
        
        // Sort files by path within each group for consistent output
        files.sort_by(|a, b| a.path.cmp(&b.path));
        
        for entry in files {
            let display_path = make_relative_to_current(&repo_root, &current_dir, &entry.path)?;
            let mut display_entry = entry.clone();
            display_entry.path = display_path;
            println!("  {}", file_utils::format_entry(&display_entry));
        }
        println!();
    }
    
    Ok(())
}

/// Prune files that exist in another index
pub fn prune(source: Option<String>, purge: bool, restore: bool, force: bool, no_ignore: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    
    if restore {
        // Restore pruned files
        let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");
        
        if !pruneyard_path.exists() {
            println!("No pruneyard directory exists");
            return Ok(());
        }
        
        let mut index = Index::load(&repo_root)?;
        let mut restored_count = 0;
        
        // Walk through pruneyard and restore files
        for entry in WalkDir::new(&pruneyard_path) {
            let entry = entry?;
            
            if entry.file_type().is_file() {
                let rel_from_pruneyard = entry.path().strip_prefix(&pruneyard_path)
                    .context("Failed to get relative path from pruneyard")?;
                let original_path = repo_root.join(rel_from_pruneyard);
                
                // Create parent directories if needed
                if let Some(parent) = original_path.parent() {
                    fs::create_dir_all(parent)
                        .context(format!("Failed to create directory: {}", parent.display()))?;
                }
                
                // Move file back to original location
                fs::rename(entry.path(), &original_path)
                    .context(format!("Failed to restore file: {}", entry.path().display()))?;
                
                // Add back to index
                let rel_path_str = rel_from_pruneyard.to_string_lossy().to_string();
                let file_entry = file_utils::create_file_entry(&original_path, rel_path_str)?;
                index.upsert(file_entry)?;
                
                println!("Restored: {}", rel_from_pruneyard.display());
                restored_count += 1;
            }
        }
        
        // Remove empty pruneyard directory
        if restored_count > 0 {
            fs::remove_dir_all(&pruneyard_path)
                .context("Failed to remove pruneyard directory")?;
        }
        
        index.save(&repo_root)?;
        
        println!("Restored {} file(s) from pruneyard", restored_count);
        return Ok(());
    }
    
    if purge {
        // Check for pending changes in local index before purging
        if has_pending_changes(&repo_root)? {
            bail!("Cannot purge: there are pending changes in the local index. Run 'oci status' to see changes.");
        }
        
        // Permanently delete pruned files
        let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");
        
        if !pruneyard_path.exists() {
            println!("No pruneyard directory exists");
            return Ok(());
        }
        
        let count = count_files_in_dir(&pruneyard_path)?;
        
        // Ask for confirmation unless --force is used
        if !force {
            println!("This will permanently delete {} pruned file(s).", count);
            print!("Are you sure you want to continue? (y/N): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            
            let confirmed = input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes");
            
            if !confirmed {
                println!("Purge cancelled");
                return Ok(());
            }
        }
        
        fs::remove_dir_all(&pruneyard_path)
            .context("Failed to remove pruneyard directory")?;
        
        println!("Permanently deleted {} pruned file(s)", count);
        return Ok(());
    }
    
    // Need source path for prune operation
    let source_path = source
        .ok_or_else(|| anyhow::anyhow!("Source path is required for prune operation"))?;
    
    // Check for pending changes in local index
    if has_pending_changes(&repo_root)? {
        bail!("Cannot prune: there are pending changes in the local index. Run 'oci status' to see changes.");
    }
    
    // Load local and source indices
    let mut local_index = Index::load(&repo_root)?;
    
    let source_abs_path = if Path::new(&source_path).is_absolute() {
        PathBuf::from(&source_path)
    } else {
        env::current_dir()?.join(&source_path)
    };
    
    if !source_abs_path.exists() {
        bail!("Source path does not exist: {}", source_abs_path.display());
    }
    
    // Canonicalize both paths to compare them properly
    let canonical_source = source_abs_path.canonicalize()
        .context("Failed to canonicalize source path")?;
    let canonical_local = repo_root.canonicalize()
        .context("Failed to canonicalize local path")?;
    
    if canonical_source == canonical_local {
        bail!("Cannot prune using the same index as source and local");
    }
    
    // Check for pending changes in source index
    if has_pending_changes(&source_abs_path)? {
        bail!("Cannot prune: there are pending changes in the source index at {}. Run 'oci status' in the source directory to see changes.", source_abs_path.display());
    }
    
    let source_index = Index::load(&source_abs_path)
        .context("Failed to load source index")?;
    
    // Load source ignore patterns if not disabled
    let source_patterns = if !no_ignore {
        ignore::load_patterns(&source_abs_path)?
    } else {
        Vec::new()
    };
    
    // Get all files from local index
    let local_files = local_index.get_dir_files_recursive("")?;
    
    // Find files to prune - store as (path, reason, in_index)
    let mut files_to_prune: Vec<(String, String, bool)> = Vec::new();
    
    for local_entry in &local_files {
        let mut should_prune = false;
        let mut prune_reason = String::new();
        
        // Check if hash exists in source index
        let source_matches = source_index.find_by_hash(&local_entry.sha256)?;
        if !source_matches.is_empty() {
            should_prune = true;
            prune_reason = "duplicate".to_string();
        }
        
        // Check if file matches source ignore patterns (unless --no-ignore)
        if !no_ignore && !source_patterns.is_empty() {
            let path = Path::new(&local_entry.path);
            if ignore::should_ignore(path, &source_patterns) {
                should_prune = true;
                prune_reason = "ignored".to_string();
            }
        }
        
        if should_prune {
            files_to_prune.push((local_entry.path.clone(), prune_reason, true));
        }
    }
    
    // Also check for files on filesystem that match source ignore patterns but aren't in local index
    if !no_ignore && !source_patterns.is_empty() {
        for entry in WalkDir::new(&repo_root).into_iter()
            .filter_entry(|e| {
                // Don't walk into .oci directory
                if let Ok(rel) = e.path().strip_prefix(&repo_root) {
                    let rel_str = rel.to_string_lossy();
                    !rel_str.starts_with(".oci")
                } else {
                    true
                }
            }) {
            let entry = entry?;
            
            if entry.file_type().is_file() {
                let rel_path = entry.path().strip_prefix(&repo_root)
                    .context("Path is outside repository")?;
                let rel_path_str = rel_path.to_string_lossy().to_string();
                
                // Skip if already in our prune list
                if files_to_prune.iter().any(|(p, _, _)| p == &rel_path_str) {
                    continue;
                }
                
                // Skip if in local index (we already checked those above)
                if local_index.get(&rel_path_str)?.is_some() {
                    continue;
                }
                
                // Check if file matches source ignore patterns
                if ignore::should_ignore(rel_path, &source_patterns) {
                    files_to_prune.push((rel_path_str, "ignored".to_string(), false));
                }
            }
        }
    }
    
    if files_to_prune.is_empty() {
        println!("No files to prune");
        return Ok(());
    }
    
    // Create pruneyard directory
    let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");
    fs::create_dir_all(&pruneyard_path)
        .context("Failed to create pruneyard directory")?;
    
    let mut pruned_count = 0;
    let mut duplicate_count = 0;
    let mut ignored_count = 0;
    
    // Move files to pruneyard
    for (path, reason, in_index) in files_to_prune {
        let source_file = repo_root.join(&path);
        let dest_file = pruneyard_path.join(&path);
        
        // Create parent directories in pruneyard
        if let Some(parent) = dest_file.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create directory: {}", parent.display()))?;
        }
        
        // Move the file
        fs::rename(&source_file, &dest_file)
            .context(format!("Failed to move file: {}", source_file.display()))?;
        
        // Remove empty parent directories
        remove_empty_dirs(&source_file, &repo_root)?;
        
        // Remove from index if it was in the index
        if in_index {
            local_index.remove(&path)?;
        }
        
        println!("Pruned ({}): {}", reason, path);
        pruned_count += 1;
        
        if reason == "duplicate" {
            duplicate_count += 1;
        } else if reason == "ignored" {
            ignored_count += 1;
        }
    }
    
    local_index.save(&repo_root)?;
    
    // Clean up any remaining empty directories
    let empty_dirs_removed = remove_all_empty_dirs(&repo_root)?;
    
    if pruned_count > 0 {
        println!("Pruned {} file(s) to .oci/pruneyard/ ({} duplicates, {} ignored)", 
                 pruned_count, duplicate_count, ignored_count);
    } else {
        println!("Pruned 0 file(s)");
    }
    
    if empty_dirs_removed > 0 {
        println!("Removed {} empty director{}", empty_dirs_removed, if empty_dirs_removed == 1 { "y" } else { "ies" });
    }
    
    Ok(())
}

/// Remove the index (deinitialize)
pub fn deinit(force: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    let oci_dir = repo_root.join(OCI_DIR);
    
    // Ask for confirmation unless --force is used
    if !force {
        println!("This will permanently delete the index at {}", oci_dir.display());
        print!("Are you sure you want to continue? (y/N): ");
        std::io::Write::flush(&mut std::io::stdout())?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        let confirmed = input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes");
        
        if !confirmed {
            println!("Deinit cancelled");
            return Ok(());
        }
    }
    
    fs::remove_dir_all(&oci_dir)
        .context("Failed to remove .oci directory")?;
    
    println!("Deinitialized oci index at {}", oci_dir.display());
    Ok(())
}

/// Show index statistics
pub fn stats() -> Result<()> {
    let repo_root = find_repo_root()?;
    let index = Index::load(&repo_root)?;
    
    // Get all files from the index
    let all_files = index.get_dir_files_recursive("")?;
    
    if all_files.is_empty() {
        println!("Index is empty");
        return Ok(());
    }
    
    // Calculate statistics
    let total_files = all_files.len();
    let total_size: u64 = all_files.iter().map(|f| f.num_bytes).sum();
    
    // Group files by hash to find unique hashes and duplicates
    let mut hash_map: std::collections::HashMap<String, Vec<&crate::index::FileEntry>> = 
        std::collections::HashMap::new();
    
    for entry in &all_files {
        hash_map.entry(entry.sha256.clone())
            .or_default()
            .push(entry);
    }
    
    let unique_hashes = hash_map.len();
    
    // Calculate duplicate files (count all files in groups with >1 file)
    let duplicate_files: usize = hash_map.values()
        .filter(|files| files.len() > 1)
        .map(|files| files.len())
        .sum();
    
    // Calculate unique size (sum of sizes for one file per hash)
    let unique_size: u64 = hash_map.values()
        .map(|files| files[0].num_bytes)
        .sum();
    
    // Calculate wasted space (duplicates)
    let wasted_space: u64 = hash_map.values()
        .filter(|files| files.len() > 1)
        .map(|files| {
            let file_size = files[0].num_bytes;
            file_size * (files.len() as u64 - 1)
        })
        .sum();
    
    // Calculate storage efficiency (how much space is actual unique content)
    let storage_efficiency = if total_size > 0 {
        (unique_size as f64 / total_size as f64) * 100.0
    } else {
        100.0
    };
    
    // Display statistics
    println!("Index Statistics:");
    println!("  Total files: {}", total_files);
    println!("  Total size: {} bytes ({:.2} MB)", total_size, total_size as f64 / 1_048_576.0);
    println!("  Unique hashes: {}", unique_hashes);
    println!("  Duplicate files: {}", duplicate_files);
    
    if duplicate_files > 0 {
        let duplicate_groups = hash_map.values().filter(|files| files.len() > 1).count();
        println!("  Duplicate groups: {}", duplicate_groups);
        println!("  Wasted space: {} bytes ({:.2} MB)", wasted_space, wasted_space as f64 / 1_048_576.0);
    }
    
    println!("  Storage efficiency: {:.2}%", storage_efficiency);
    
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

/// Check if there are any pending changes in the repository
fn has_pending_changes(repo_root: &Path) -> Result<bool> {
    let index = Index::load(repo_root)?;
    let patterns = ignore::load_patterns(repo_root)?;
    
    // Get all files from filesystem
    let mut fs_files = std::collections::HashSet::new();
    
    for entry in WalkDir::new(repo_root).into_iter()
        .filter_entry(|e| {
            // Convert to relative path for pattern matching
            if let Ok(rel) = e.path().strip_prefix(repo_root) {
                !ignore::should_ignore(rel, &patterns)
            } else {
                true // Don't filter if path conversion fails
            }
        }) {
        let entry = entry?;
        
        if entry.file_type().is_file() {
            let rel_path = entry.path().strip_prefix(repo_root)
                .context("Path is outside repository")?;
            fs_files.insert(rel_path.to_string_lossy().to_string());
        }
    }
    
    // Get all indexed files
    let indexed_files = index.get_dir_files_recursive("")?;
    
    // Check for modified or added files
    for fs_path in &fs_files {
        let full_path = repo_root.join(fs_path);
        
        if let Some(entry) = index.get(fs_path)? {
            // File exists in index - check if modified
            if file_utils::has_changed(&entry, &full_path)? {
                return Ok(true);
            }
        } else {
            // File not in index - added
            return Ok(true);
        }
    }
    
    // Check for deleted files
    for entry in indexed_files {
        if !fs_files.contains(&entry.path) {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// Count files in a directory recursively
fn count_files_in_dir(dir: &Path) -> Result<usize> {
    let mut count = 0;
    
    for entry in WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            count += 1;
        }
    }
    
    Ok(count)
}

/// Remove empty parent directories recursively up to the repo root
fn remove_empty_dirs(file_path: &Path, repo_root: &Path) -> Result<()> {
    if let Some(mut parent) = file_path.parent() {
        // Walk up the directory tree
        while parent != repo_root && parent.starts_with(repo_root) {
            // Try to read the directory
            match fs::read_dir(parent) {
                Ok(mut entries) => {
                    // Check if directory is empty
                    if entries.next().is_none() {
                        // Directory is empty, remove it
                        fs::remove_dir(parent)
                            .context(format!("Failed to remove empty directory: {}", parent.display()))?;
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
fn remove_all_empty_dirs(repo_root: &Path) -> Result<usize> {
    let mut removed_count = 0;
    let mut found_empty = true;
    
    // Keep scanning until no more empty directories are found
    // (needed because removing a directory might make its parent empty)
    while found_empty {
        found_empty = false;
        let mut dirs_to_remove = Vec::new();
        
        // Collect all directories (walk depth-first, post-order)
        // We need to collect them first, then remove, to avoid iterator invalidation
        for entry in WalkDir::new(repo_root)
            .min_depth(1)
            .contents_first(true) {
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
            if let Ok(()) = fs::remove_dir(&dir) {
                removed_count += 1;
                found_empty = true;
            }
        }
    }
    
    Ok(removed_count)
}
