use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::file_utils;
use crate::ignore;
use crate::index::{Index, OCI_DIR};
use crate::config::Config;
use crate::scanner::FileScanner;
use crate::display::{DisplayContext, StatusMarker};
use crate::dir_utils;

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

/// Check the version of the index and warn if it doesn't match the tool version
fn check_version(repo_root: &Path) -> Result<()> {
    let config = Config::load(repo_root)?;
    if !config.check_version() {
        config.warn_version_mismatch();
    }
    Ok(())
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
    
    // Initialize config with current version
    let config = Config::new();
    config.save(&current_dir)?;
    
    // Initialize ignore with default patterns
    ignore::init_ignore_file(&current_dir)?;
    
    println!("Initialized empty oci index in {}", oci_dir.display());
    Ok(())
}

/// Add a pattern to the ignore list
pub fn ignore(pattern: Option<String>) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
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
    println!("Added pattern to ignore: {}", pattern_to_add);
    
    Ok(())
}

/// Determine what to scan based on status command arguments
fn determine_scan_target(
    pattern: Option<String>,
    recursive: bool,
    repo_root: &Path,
    current_dir: &Path,
) -> Result<(PathBuf, String, bool)> {
    if let Some(p) = pattern {
        // Path argument provided
        let target_path = current_dir.join(&p);
        if !target_path.exists() {
            bail!("Path does not exist: {}", target_path.display());
        }

        // Canonicalize to resolve ".", "..", and symlinks
        let canonical_path = target_path
            .canonicalize()
            .context("Failed to canonicalize path")?;

        let rel_path = canonical_path
            .strip_prefix(&repo_root.canonicalize()?)
            .context("Path is outside repository")?;
        let rel_path_str = rel_path.to_string_lossy().to_string();

        // If it's a file, always non-recursive; if directory, use recursive flag
        let is_recursive = canonical_path.is_dir() && recursive;
        Ok((canonical_path, rel_path_str, is_recursive))
    } else if recursive {
        // No path, but -r flag: scan from current directory recursively
        let rel_current = current_dir
            .strip_prefix(repo_root)
            .context("Current directory is outside repository")?;
        Ok((
            current_dir.to_path_buf(),
            rel_current.to_string_lossy().to_string(),
            true,
        ))
    } else {
        // No path, no -r flag: scan entire repository from root
        Ok((repo_root.to_path_buf(), String::new(), true))
    }
}

/// Scan the filesystem and collect file information
fn scan_filesystem_for_status(
    scan_dir: &Path,
    is_recursive: bool,
    repo_root: &Path,
    patterns: &[String],
    verbose: bool,
) -> Result<(std::collections::HashSet<String>, std::collections::HashSet<String>)> {
    let mut fs_files = std::collections::HashSet::new();
    let mut ignored_files = std::collections::HashSet::new();

    if scan_dir.is_file() {
        // Single file
        let rel_path = scan_dir
            .strip_prefix(repo_root)
            .context("Path is outside repository")?;
        let rel_path_str = rel_path.to_string_lossy().to_string();

        if ignore::should_ignore(rel_path, patterns) {
            if verbose {
                ignored_files.insert(rel_path_str);
            }
        } else {
            fs_files.insert(rel_path_str);
        }
    } else {
        // Directory - need to walk without filtering for verbose mode
        let walker = if is_recursive {
            WalkDir::new(scan_dir).into_iter()
        } else {
            WalkDir::new(scan_dir).max_depth(1).into_iter()
        };

        for entry in walker {
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
                    .strip_prefix(repo_root)
                    .context("Path is outside repository")?;
                let rel_path_str = rel_path.to_string_lossy().to_string();

                if ignore::should_ignore(rel_path, patterns) {
                    if verbose {
                        ignored_files.insert(rel_path_str);
                    }
                } else {
                    fs_files.insert(rel_path_str);
                }
            }
        }
    }

    Ok((fs_files, ignored_files))
}

/// Display status changes between filesystem and index
fn display_status_changes(
    fs_files: &std::collections::HashSet<String>,
    indexed_files: Vec<crate::index::FileEntry>,
    ignored_files: &std::collections::HashSet<String>,
    repo_root: &Path,
    display_ctx: &DisplayContext,
    index: &Index,
    verbose: bool,
) -> Result<bool> {
    let mut has_changes = false;

    // Check for modified, added, and unchanged files
    for fs_path in fs_files {
        let full_path = repo_root.join(fs_path);

        if let Some(entry) = index.get(fs_path)? {
            // File exists in index - check if modified
            if file_utils::has_changed(&entry, &full_path)? {
                let display_path = display_ctx.make_relative(fs_path)?;
                let display_entry = display_ctx.create_display_entry(&full_path, display_path)?;
                StatusMarker::Updated.display(&file_utils::format_entry(&display_entry));
                has_changes = true;
            } else if verbose {
                // Unchanged file - only show in verbose mode
                let display_path = display_ctx.make_relative(fs_path)?;
                let display_entry = display_ctx.create_display_entry(&full_path, display_path)?;
                StatusMarker::Unchanged.display(&file_utils::format_entry(&display_entry));
            }
        } else {
            // File not in index - added
            let display_path = display_ctx.make_relative(fs_path)?;
            let display_entry = display_ctx.create_display_entry(&full_path, display_path)?;
            StatusMarker::Added.display(&file_utils::format_entry(&display_entry));
            has_changes = true;
        }
    }

    // Check for deleted files
    for entry in indexed_files {
        if !fs_files.contains(&entry.path) {
            let formatted = display_ctx.format_entry_relative(&entry)?;
            StatusMarker::Deleted.display(&formatted);
            has_changes = true;
        }
    }

    // Show ignored files in verbose mode
    if verbose {
        for ignored_path in ignored_files {
            let full_path = repo_root.join(ignored_path);
            if full_path.exists() {
                let display_path = display_ctx.make_relative(ignored_path)?;
                let display_entry = display_ctx.create_display_entry(&full_path, display_path)?;
                StatusMarker::Ignored.display(&file_utils::format_entry(&display_entry));
            }
        }
    }

    Ok(has_changes)
}

/// Check status of files
pub fn status(pattern: Option<String>, recursive: bool, verbose: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;
    let patterns = ignore::load_patterns(&repo_root)?;

    // Determine what to scan based on arguments
    let (scan_dir, scan_rel_path, is_recursive) =
        determine_scan_target(pattern, recursive, &repo_root, &current_dir)?;

    // Scan filesystem
    let (fs_files, ignored_files) =
        scan_filesystem_for_status(&scan_dir, is_recursive, &repo_root, &patterns, verbose)?;

    // Get indexed files for comparison
    let indexed_files: Vec<_> = if is_recursive {
        index.get_dir_files_recursive(&scan_rel_path)?
    } else {
        index.get_dir_files(&scan_rel_path)?
    };

    // Display changes
    let display_ctx = DisplayContext::new(repo_root.clone(), current_dir);
    let has_changes = display_status_changes(
        &fs_files,
        indexed_files,
        &ignored_files,
        &repo_root,
        &display_ctx,
        &index,
        verbose,
    )?;

    if !verbose && !has_changes {
        println!("No changes");
    }

    Ok(())
}

/// Update statistics tracker
struct UpdateStats {
    added_count: usize,
    updated_count: usize,
    removed_count: usize,
    skipped_count: usize,
}

impl UpdateStats {
    fn new() -> Self {
        Self {
            added_count: 0,
            updated_count: 0,
            removed_count: 0,
            skipped_count: 0,
        }
    }

    fn print_summary(&self) {
        let total_changed = self.added_count + self.updated_count + self.removed_count;
        if total_changed > 0 {
            println!(
                "Updated {} file(s) in the index ({} added, {} updated, {} removed)",
                total_changed, self.added_count, self.updated_count, self.removed_count
            );
        } else {
            println!("Updated 0 file(s) in the index");
        }

        if self.skipped_count > 0 {
            println!("Skipped {} unchanged file(s)", self.skipped_count);
        }
    }
}

/// Update a single file in the index
fn update_single_file(
    index: &mut Index,
    target_path: &Path,
    repo_root: &Path,
    display_ctx: &DisplayContext,
    patterns: &[String],
    verbose: bool,
    stats: &mut UpdateStats,
) -> Result<()> {
    let rel_path = target_path
        .strip_prefix(repo_root)
        .context("Path is outside repository")?;
    let rel_path_str = rel_path.to_string_lossy().to_string();

    if ignore::should_ignore(rel_path, patterns) {
        // File is ignored
        if verbose {
            let display_path = display_ctx.make_relative(&rel_path_str)?;
            StatusMarker::Ignored.display(&display_path);
        }
    } else {
        let is_new = index.get(&rel_path_str)?.is_none();

        if should_update_file(index, target_path, &rel_path_str)? {
            let display_path = display_ctx.make_relative(&rel_path_str)?;
            let marker = if is_new {
                StatusMarker::Added
            } else {
                StatusMarker::Updated
            };
            marker.display(&display_path);

            let entry = file_utils::create_file_entry(target_path, rel_path_str)?;
            index.upsert(entry)?;

            if is_new {
                stats.added_count += 1;
            } else {
                stats.updated_count += 1;
            }
        } else {
            stats.skipped_count += 1;
            if verbose {
                let display_path = display_ctx.make_relative(&rel_path_str)?;
                StatusMarker::Unchanged.display(&display_path);
            }
        }
    }

    Ok(())
}

/// Update all files in a directory recursively
fn update_directory(
    index: &mut Index,
    target_path: &Path,
    repo_root: &Path,
    display_ctx: &DisplayContext,
    patterns: &[String],
    verbose: bool,
    stats: &mut UpdateStats,
) -> Result<()> {
    let mut fs_files = std::collections::HashSet::new();
    let mut ignored_files: Vec<String> = Vec::new();

    // Walk the directory tree
    for entry in WalkDir::new(target_path).into_iter().filter_entry(|e| {
        // In verbose mode, we want to see ignored files too,
        // so we need to walk into directories even if they match ignore patterns
        // But we still skip .oci directory
        if let Ok(rel) = e.path().strip_prefix(repo_root) {
            let rel_str = rel.to_string_lossy();
            !rel_str.starts_with(".oci")
        } else {
            true
        }
    }) {
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
                .strip_prefix(repo_root)
                .context("Path is outside repository")?;
            let rel_path_str = rel_path.to_string_lossy().to_string();

            if ignore::should_ignore(rel_path, patterns) {
                // File is ignored - only collect if verbose
                if verbose {
                    ignored_files.push(rel_path_str);
                }
            } else {
                fs_files.insert(rel_path_str.clone());

                let is_new = index.get(&rel_path_str)?.is_none();

                if should_update_file(index, entry.path(), &rel_path_str)? {
                    let display_path = display_ctx.make_relative(&rel_path_str)?;
                    let marker = if is_new {
                        StatusMarker::Added
                    } else {
                        StatusMarker::Updated
                    };
                    marker.display(&display_path);

                    let file_entry = file_utils::create_file_entry(entry.path(), rel_path_str)?;
                    index.upsert(file_entry)?;

                    if is_new {
                        stats.added_count += 1;
                    } else {
                        stats.updated_count += 1;
                    }
                } else {
                    stats.skipped_count += 1;
                    if verbose {
                        let display_path = display_ctx.make_relative(&rel_path_str)?;
                        StatusMarker::Unchanged.display(&display_path);
                    }
                }
            }
        }
    }

    // Now check for deleted files in the index
    let rel_target = target_path
        .strip_prefix(repo_root)
        .context("Path is outside repository")?;
    let rel_target_str = rel_target.to_string_lossy().to_string();

    let indexed_files = index.get_dir_files_recursive(&rel_target_str)?;

    for indexed_entry in indexed_files {
        if !fs_files.contains(&indexed_entry.path) {
            // File is in index but not on disk - remove it
            let display_path = display_ctx.make_relative(&indexed_entry.path)?;
            StatusMarker::Deleted.display(&display_path);
            index.remove(&indexed_entry.path)?;
            stats.removed_count += 1;
        }
    }

    // Display ignored files if verbose
    if verbose {
        for rel_path_str in ignored_files {
            let display_path = display_ctx.make_relative(&rel_path_str)?;
            StatusMarker::Ignored.display(&display_path);
        }
    }

    Ok(())
}

/// Update the index with changes from the filesystem
pub fn update(pattern: Option<String>, verbose: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
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

    // Canonicalize to resolve ".", "..", and symlinks
    let target_path = target_path
        .canonicalize()
        .context("Failed to canonicalize path")?;

    let display_ctx = DisplayContext::new(repo_root.clone(), current_dir);
    let mut stats = UpdateStats::new();

    if target_path.is_file() {
        update_single_file(
            &mut index,
            &target_path,
            &repo_root,
            &display_ctx,
            &patterns,
            verbose,
            &mut stats,
        )?;
    } else {
        update_directory(
            &mut index,
            &target_path,
            &repo_root,
            &display_ctx,
            &patterns,
            verbose,
            &mut stats,
        )?;
    }

    index.save(&repo_root)?;
    stats.print_summary();

    Ok(())
}

/// List files in the index
pub fn ls(recursive: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;

    let rel_current = current_dir
        .strip_prefix(&repo_root)
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

    let display_ctx = DisplayContext::new(repo_root, current_dir);
    for entry in entries {
        let formatted = display_ctx.format_entry_relative(&entry)?;
        println!("{}", formatted);
    }

    Ok(())
}

/// Find files by hash
pub fn grep(hash: &str) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
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
    check_version(&repo_root)?;
    let current_dir = env::current_dir()?;
    let index = Index::load(&repo_root)?;

    // Get all files from the repository recursively
    let entries: Vec<_> = index.get_dir_files_recursive("")?;

    // Group files by hash
    let mut hash_groups: std::collections::HashMap<String, Vec<crate::index::FileEntry>> =
        std::collections::HashMap::new();

    for entry in entries {
        hash_groups
            .entry(entry.sha256.clone())
            .or_default()
            .push(entry);
    }

    // Filter to only hashes with duplicates (more than 1 file)
    let mut duplicate_groups: Vec<_> = hash_groups
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .collect();

    if duplicate_groups.is_empty() {
        println!("No duplicate files found");
        return Ok(());
    }

    // Sort groups by hash for consistent output
    duplicate_groups.sort_by(|a, b| a.0.cmp(&b.0));

    // Calculate statistics
    let total_duplicate_files: usize =
        duplicate_groups.iter().map(|(_, files)| files.len()).sum();
    let total_groups = duplicate_groups.len();

    // Calculate wasted space (all but one copy of each duplicate set)
    let wasted_bytes: u64 = duplicate_groups
        .iter()
        .map(|(_, files)| {
            let file_size = files[0].num_bytes;
            file_size * (files.len() as u64 - 1)
        })
        .sum();

    println!(
        "Found {} duplicate file(s) in {} group(s)",
        total_duplicate_files, total_groups
    );
    println!(
        "Potential space savings: {} bytes ({:.2} MB)\n",
        wasted_bytes,
        wasted_bytes as f64 / 1_048_576.0
    );

    // Display each group
    let display_ctx = DisplayContext::new(repo_root, current_dir);
    for (hash, mut files) in duplicate_groups {
        println!("Hash: {}", hash);

        // Sort files by path within each group for consistent output
        files.sort_by(|a, b| a.path.cmp(&b.path));

        for entry in files {
            let formatted = display_ctx.format_entry_relative(&entry)?;
            println!("  {}", formatted);
        }
        println!();
    }

    Ok(())
}

/// Restore files from pruneyard back to their original locations
fn prune_restore(repo_root: &Path) -> Result<()> {
    let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");

    if !pruneyard_path.exists() {
        println!("No pruneyard directory exists");
        return Ok(());
    }

    let mut index = Index::load(repo_root)?;
    let mut restored_count = 0;

    // Walk through pruneyard and restore files
    for entry in WalkDir::new(&pruneyard_path) {
        let entry = entry?;

        if entry.file_type().is_file() {
            let rel_from_pruneyard = entry
                .path()
                .strip_prefix(&pruneyard_path)
                .context("Failed to get relative path from pruneyard")?;
            let original_path = repo_root.join(rel_from_pruneyard);

            // Create parent directories if needed
            if let Some(parent) = original_path.parent() {
                fs::create_dir_all(parent)
                    .context(format!("Failed to create directory: {}", parent.display()))?;
            }

            // Move file back to original location
            fs::rename(entry.path(), &original_path).context(format!(
                "Failed to restore file: {}",
                entry.path().display()
            ))?;

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

    index.save(repo_root)?;

    println!("Restored {} file(s) from pruneyard", restored_count);
    Ok(())
}

/// Permanently delete all files in pruneyard
fn prune_purge(repo_root: &Path, force: bool) -> Result<()> {
    // Check for pending changes in local index before purging
    if has_pending_changes(repo_root)? {
        bail!("Cannot purge: there are pending changes in the local index. Run 'oci status' to see changes.");
    }

    let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");

    if !pruneyard_path.exists() {
        println!("No pruneyard directory exists");
        return Ok(());
    }

    let count = dir_utils::count_files_in_dir(&pruneyard_path)?;

    // Ask for confirmation unless --force is used
    if !force {
        println!("This will permanently delete {} pruned file(s).", count);
        print!("Are you sure you want to continue? (y/N): ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        let confirmed =
            input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes");

        if !confirmed {
            println!("Purge cancelled");
            return Ok(());
        }
    }

    fs::remove_dir_all(&pruneyard_path).context("Failed to remove pruneyard directory")?;

    println!("Permanently deleted {} pruned file(s)", count);
    Ok(())
}

/// Find files to prune based on source index and ignore patterns
fn find_files_to_prune(
    local_index: &Index,
    source_index: &Index,
    repo_root: &Path,
    source_patterns: &[String],
    local_patterns: &[String],
    no_ignore: bool,
    ignored: bool,
) -> Result<Vec<(String, String, bool)>> {
    let mut files_to_prune: Vec<(String, String, bool)> = Vec::new();

    // Get all files from local index
    let local_files = local_index.get_dir_files_recursive("")?;

    // Check indexed files
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
            if ignore::should_ignore(path, source_patterns) {
                should_prune = true;
                prune_reason = "ignored".to_string();
            }
        }

        // Check if file matches local ignore patterns (if --ignored flag is present)
        if ignored && !local_patterns.is_empty() {
            let path = Path::new(&local_entry.path);
            if ignore::should_ignore(path, local_patterns) {
                should_prune = true;
                prune_reason = "ignored".to_string();
            }
        }

        if should_prune {
            files_to_prune.push((local_entry.path.clone(), prune_reason, true));
        }
    }

    // Also check for files on filesystem that match ignore patterns but aren't in local index
    let check_fs_ignored =
        (!no_ignore && !source_patterns.is_empty()) || (ignored && !local_patterns.is_empty());
    if check_fs_ignored {
        for entry in WalkDir::new(repo_root).into_iter().filter_entry(|e| {
            // Don't walk into .oci directory
            if let Ok(rel) = e.path().strip_prefix(repo_root) {
                let rel_str = rel.to_string_lossy();
                !rel_str.starts_with(".oci")
            } else {
                true
            }
        }) {
            // Handle permission errors gracefully - skip and continue
            let entry = match entry {
                Ok(e) => e,
                Err(_err) => {
                    // Silently skip permission errors during prune
                    continue;
                }
            };

            if entry.file_type().is_file() {
                let rel_path = entry
                    .path()
                    .strip_prefix(repo_root)
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
                if !no_ignore && ignore::should_ignore(rel_path, source_patterns) {
                    files_to_prune.push((rel_path_str.clone(), "ignored".to_string(), false));
                }

                // Check if file matches local ignore patterns (if --ignored flag is present)
                if ignored && ignore::should_ignore(rel_path, local_patterns) {
                    // Only add if not already in list
                    if !files_to_prune.iter().any(|(p, _, _)| p == &rel_path_str) {
                        files_to_prune.push((rel_path_str, "ignored".to_string(), false));
                    }
                }
            }
        }
    }

    Ok(files_to_prune)
}

/// Execute the prune by moving files to pruneyard
fn execute_prune(
    files_to_prune: Vec<(String, String, bool)>,
    local_index: &mut Index,
    repo_root: &Path,
) -> Result<(usize, usize, usize)> {
    let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");
    fs::create_dir_all(&pruneyard_path).context("Failed to create pruneyard directory")?;

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
        dir_utils::remove_empty_parent_dirs(&source_file, repo_root)?;

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

    Ok((pruned_count, duplicate_count, ignored_count))
}

/// Prune files that exist in another index
pub fn prune(
    source: Option<String>,
    purge: bool,
    restore: bool,
    force: bool,
    no_ignore: bool,
    ignored: bool,
) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;

    // Handle restore flag
    if restore {
        return prune_restore(&repo_root);
    }

    // Handle purge flag
    if purge {
        return prune_purge(&repo_root, force);
    }

    // Check for pending changes in local index
    if has_pending_changes(&repo_root)? {
        bail!("Cannot prune: there are pending changes in the local index. Run 'oci status' to see changes.");
    }

    // If --ignored flag is present without a source, just prune local ignored files
    if ignored && source.is_none() {
        return prune_local_ignored_files(&repo_root);
    }

    // Need source path for prune operation (unless only using --ignored)
    let source_path = source.ok_or_else(|| {
        anyhow::anyhow!(
            "Source path is required for prune operation (unless using --ignored without source)"
        )
    })?;

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
    let canonical_source = source_abs_path
        .canonicalize()
        .context("Failed to canonicalize source path")?;
    let canonical_local = repo_root
        .canonicalize()
        .context("Failed to canonicalize local path")?;

    if canonical_source == canonical_local {
        bail!("Cannot prune using the same index as source and local");
    }

    // Check for pending changes in source index
    if has_pending_changes(&source_abs_path)? {
        bail!(
            "Cannot prune: there are pending changes in the source index at {}. Run 'oci status' in the source directory to see changes.",
            source_abs_path.display()
        );
    }

    let source_index = Index::load(&source_abs_path).context("Failed to load source index")?;

    // Load source ignore patterns if not disabled
    let source_patterns = if !no_ignore {
        ignore::load_patterns(&source_abs_path)?
    } else {
        Vec::new()
    };

    // Load local ignore patterns if --ignored flag is present
    let local_patterns = if ignored {
        ignore::load_patterns(&repo_root)?
    } else {
        Vec::new()
    };

    // Find files to prune
    let files_to_prune = find_files_to_prune(
        &local_index,
        &source_index,
        &repo_root,
        &source_patterns,
        &local_patterns,
        no_ignore,
        ignored,
    )?;

    if files_to_prune.is_empty() {
        println!("No files to prune");
        return Ok(());
    }

    // Execute prune
    let (pruned_count, duplicate_count, ignored_count) =
        execute_prune(files_to_prune, &mut local_index, &repo_root)?;

    local_index.save(&repo_root)?;

    // Clean up any remaining empty directories
    let empty_dirs_removed = dir_utils::remove_all_empty_dirs(&repo_root)?;

    if pruned_count > 0 {
        println!(
            "Pruned {} file(s) to .oci/pruneyard/ ({} duplicates, {} ignored)",
            pruned_count, duplicate_count, ignored_count
        );
    } else {
        println!("Pruned 0 file(s)");
    }

    if empty_dirs_removed > 0 {
        println!(
            "Removed {} empty director{}",
            empty_dirs_removed,
            if empty_dirs_removed == 1 { "y" } else { "ies" }
        );
    }

    Ok(())
}

/// Remove the index (deinitialize)
pub fn deinit(force: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    check_version(&repo_root)?;
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
    check_version(&repo_root)?;
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

/// Prune files matching local ignore patterns
fn prune_local_ignored_files(repo_root: &Path) -> Result<()> {
    let mut local_index = Index::load(repo_root)?;
    let local_patterns = ignore::load_patterns(repo_root)?;
    
    if local_patterns.is_empty() {
        println!("No ignore patterns defined in local ignore");
        return Ok(());
    }
    
    // Find files to prune - store as (path, in_index)
    let mut files_to_prune: Vec<(String, bool)> = Vec::new();
    
    // Check files in the index
    let local_files = local_index.get_dir_files_recursive("")?;
    for local_entry in &local_files {
        let path = Path::new(&local_entry.path);
        if ignore::should_ignore(path, &local_patterns) {
            files_to_prune.push((local_entry.path.clone(), true));
        }
    }
    
    // Check files on filesystem that aren't in the index
    for entry in WalkDir::new(repo_root).into_iter()
        .filter_entry(|e| {
            // Don't walk into .oci directory
            if let Ok(rel) = e.path().strip_prefix(repo_root) {
                let rel_str = rel.to_string_lossy();
                !rel_str.starts_with(".oci")
            } else {
                true
            }
        }) {
        // Handle permission errors gracefully - skip and continue
        let entry = match entry {
            Ok(e) => e,
            Err(_err) => {
                // Silently skip permission errors
                continue;
            }
        };
        
        if entry.file_type().is_file() {
            let rel_path = entry.path().strip_prefix(repo_root)
                .context("Path is outside repository")?;
            let rel_path_str = rel_path.to_string_lossy().to_string();
            
            // Skip if already in our prune list
            if files_to_prune.iter().any(|(p, _)| p == &rel_path_str) {
                continue;
            }
            
            // Skip if in local index (we already checked those above)
            if local_index.get(&rel_path_str)?.is_some() {
                continue;
            }
            
            // Check if file matches local ignore patterns
            if ignore::should_ignore(rel_path, &local_patterns) {
                files_to_prune.push((rel_path_str, false));
            }
        }
    }
    
    if files_to_prune.is_empty() {
        println!("No ignored files to prune");
        return Ok(());
    }
    
    // Create pruneyard directory
    let pruneyard_path = repo_root.join(OCI_DIR).join("pruneyard");
    fs::create_dir_all(&pruneyard_path)
        .context("Failed to create pruneyard directory")?;
    
    let mut pruned_count = 0;
    
    // Move files to pruneyard
    for (path, in_index) in files_to_prune {
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
        dir_utils::remove_empty_parent_dirs(&source_file, repo_root)?;
        
        // Remove from index if it was in the index
        if in_index {
            local_index.remove(&path)?;
        }
        
        println!("Pruned (ignored): {}", path);
        pruned_count += 1;
    }
    
    local_index.save(repo_root)?;

    // Clean up any remaining empty directories
    let empty_dirs_removed = dir_utils::remove_all_empty_dirs(repo_root)?;
    
    if pruned_count > 0 {
        println!("Pruned {} ignored file(s) to .oci/pruneyard/", pruned_count);
    } else {
        println!("Pruned 0 file(s)");
    }
    
    if empty_dirs_removed > 0 {
        println!("Removed {} empty director{}", empty_dirs_removed, if empty_dirs_removed == 1 { "y" } else { "ies" });
    }
    
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


/// Check if there are any pending changes in the repository
fn has_pending_changes(repo_root: &Path) -> Result<bool> {
    let index = Index::load(repo_root)?;
    let patterns = ignore::load_patterns(repo_root)?;

    // Use scanner to get filesystem state
    let scanner = FileScanner::new(repo_root.to_path_buf(), patterns);
    let scan_result = scanner.scan_repository_filtered(false)?;
    let fs_files = scan_result.tracked_files;

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

