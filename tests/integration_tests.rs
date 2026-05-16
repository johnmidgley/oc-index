use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

static OCI_BIN: OnceLock<PathBuf> = OnceLock::new();

fn get_oci_binary() -> &'static Path {
    OCI_BIN.get_or_init(|| {
        let output = Command::new("cargo")
            .args(&["build", "--quiet"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("Failed to build oci");

        if !output.status.success() {
            panic!("Failed to build oci binary");
        }

        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/oci")
    })
}

fn run_oci(args: &[&str], working_dir: &Path) -> (String, String, i32) {
    let output = Command::new(get_oci_binary())
        .args(args)
        .current_dir(working_dir)
        .output()
        .expect("Failed to execute oci");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

fn run_oci_with_stdin(args: &[&str], working_dir: &Path, stdin: &str) -> (String, String, i32) {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(get_oci_binary())
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn oci");

    child
        .stdin
        .as_mut()
        .expect("stdin not captured")
        .write_all(stdin.as_bytes())
        .expect("Failed to write to stdin");

    let output = child.wait_with_output().expect("Failed to wait for oci");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    (stdout, stderr, exit_code)
}

#[test]
fn test_init_creates_oci_directory() {
    let temp_dir = TempDir::new().unwrap();
    let (stdout, _, exit_code) = run_oci(&["init"], temp_dir.path());
    
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Initialized empty oci index"));
    assert!(temp_dir.path().join(".oci").exists());
    assert!(temp_dir.path().join(".oci/index.db").exists());
}

#[test]
fn test_init_fails_if_already_exists() {
    let temp_dir = TempDir::new().unwrap();
    
    // First init should succeed
    let (_, _, exit_code) = run_oci(&["init"], temp_dir.path());
    assert_eq!(exit_code, 0);
    
    // Second init should fail
    let (_, stderr, exit_code) = run_oci(&["init"], temp_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("Index already exists"));
}

#[test]
fn test_init_creates_ignore_with_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let (_, _, exit_code) = run_oci(&["init"], temp_dir.path());
    
    assert_eq!(exit_code, 0);
    
    // Verify ignore file exists
    let ignore_path = temp_dir.path().join(".oci/ignore");
    assert!(ignore_path.exists(), "ignore file should be created");
    
    // Verify it contains default patterns
    let contents = fs::read_to_string(&ignore_path).unwrap();
    assert!(contents.contains("node_modules/"), "Should contain node_modules pattern");
    assert!(contents.contains("*.pyc"), "Should contain .pyc pattern");
    assert!(contents.contains(".DS_Store"), "Should contain .DS_Store pattern");
    assert!(contents.contains("Library/Application Support/MobileSync/"), "Should contain MobileSync pattern");
    assert!(contents.contains("# Default ignore patterns"), "Should contain header comment");
}

#[test]
fn test_update_and_ls() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create test files
    fs::write(temp_dir.path().join("test1.txt"), "hello world").unwrap();
    fs::write(temp_dir.path().join("test2.txt"), "goodbye world").unwrap();
    
    // Update index with files
    let (stdout, _, exit_code) = run_oci(&["update"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 2 file(s)"));
    
    // List files
    let (stdout, _, exit_code) = run_oci(&["ls"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("test1.txt"));
    assert!(stdout.contains("test2.txt"));
}

#[test]
fn test_status_shows_changes() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create and update index with file
    fs::write(temp_dir.path().join("test.txt"), "original").unwrap();
    run_oci(&["update"], temp_dir.path());
    
    // Modify the file
    fs::write(temp_dir.path().join("test.txt"), "modified").unwrap();
    
    // Check status
    let (stdout, _, exit_code) = run_oci(&["status"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("U"));
    assert!(stdout.contains("test.txt"));
}

#[test]
fn test_grep_finds_files_by_hash() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create files with same content
    fs::write(temp_dir.path().join("file1.txt"), "same content").unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "same content").unwrap();
    run_oci(&["update"], temp_dir.path());
    
    // Get the hash from ls output
    let (stdout, _, _) = run_oci(&["ls"], temp_dir.path());
    let hash = stdout.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(2))
        .expect("Failed to extract hash");
    
    // Grep for the hash
    let (stdout, _, exit_code) = run_oci(&["grep", hash], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(stdout.contains("Found 2 file(s)"));
}

#[test]
fn test_ignore_excludes_files() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create files
    fs::write(temp_dir.path().join("include.txt"), "include me").unwrap();
    fs::write(temp_dir.path().join("exclude.log"), "exclude me").unwrap();
    
    // Add ignore pattern
    run_oci(&["ignore", "*.log"], temp_dir.path());
    
    // Update index with all files
    let (stdout, _, exit_code) = run_oci(&["update"], temp_dir.path());
    assert_eq!(exit_code, 0);
    // Should only update 1 file (the .txt file)
    assert!(stdout.contains("Updated 1 file(s)"));
    
    // List files
    let (stdout, _, _) = run_oci(&["ls"], temp_dir.path());
    assert!(stdout.contains("include.txt"));
    assert!(!stdout.contains("exclude.log"));
}

#[test]
fn test_deinit_removes_index() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    assert!(temp_dir.path().join(".oci").exists());
    
    // Deinit with -f should succeed (skips confirmation)
    let (stdout, _, exit_code) = run_oci(&["deinit", "-f"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Deinitialized"));
    assert!(!temp_dir.path().join(".oci").exists());
}

#[test]
fn test_reset_clears_index() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create and index some files
    fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
    fs::write(temp_dir.path().join("file3.txt"), "content3").unwrap();
    
    run_oci(&["update"], temp_dir.path());
    
    // Verify files are in the index
    let (stdout, _, exit_code) = run_oci(&["ls"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(stdout.contains("file3.txt"));
    
    // Stats should show 3 files
    let (stdout, _, exit_code) = run_oci(&["stats"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Total files: 3"));
    
    // Reset with -f should clear all entries
    let (stdout, _, exit_code) = run_oci(&["reset", "-f"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Reset index"));
    
    // .oci directory should still exist
    assert!(temp_dir.path().join(".oci").exists());
    assert!(temp_dir.path().join(".oci/index.db").exists());
    assert!(temp_dir.path().join(".oci/config").exists());
    assert!(temp_dir.path().join(".oci/ignore").exists());
    
    // Index should be empty
    let (stdout, _, exit_code) = run_oci(&["ls"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No files in index"));
    
    // Stats should show empty index
    let (stdout, _, exit_code) = run_oci(&["stats"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Index is empty"));
    
    // Files should still exist on filesystem
    assert!(temp_dir.path().join("file1.txt").exists());
    assert!(temp_dir.path().join("file2.txt").exists());
    assert!(temp_dir.path().join("file3.txt").exists());
    
    // Status should show all files as added (since index is empty)
    let (stdout, _, exit_code) = run_oci(&["status"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("+ "));
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(stdout.contains("file3.txt"));
}

#[test]
fn test_recursive_operations() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create nested directory structure
    fs::create_dir(temp_dir.path().join("subdir")).unwrap();
    fs::write(temp_dir.path().join("root.txt"), "root").unwrap();
    fs::write(temp_dir.path().join("subdir/nested.txt"), "nested").unwrap();
    
    // Update index with all files
    run_oci(&["update"], temp_dir.path());
    
    // List without -r should only show root.txt
    let (stdout, _, _) = run_oci(&["ls"], temp_dir.path());
    assert!(stdout.contains("root.txt"));
    assert!(!stdout.contains("nested.txt"));
    
    // List with -r should show both
    let (stdout, _, _) = run_oci(&["ls", "-r"], temp_dir.path());
    assert!(stdout.contains("root.txt"));
    assert!(stdout.contains("nested.txt"));
}

#[test]
fn test_update_skips_unchanged_files() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    // Create test files
    fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
    
    // First update - should update both files
    let (stdout, _, exit_code) = run_oci(&["update"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 2 file(s)"));
    
    // Second update without changes - should skip both files
    let (stdout, _, exit_code) = run_oci(&["update"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 0 file(s)"));
    assert!(stdout.contains("Skipped 2 unchanged file(s)"));
    
    // Modify one file
    std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure modified time changes
    fs::write(temp_dir.path().join("file1.txt"), "modified content").unwrap();
    
    // Third update - should update only the modified file
    let (stdout, _, exit_code) = run_oci(&["update"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 1 file(s)"));
    assert!(stdout.contains("Skipped 1 unchanged file(s)"));
}

#[test]
fn test_prune_moves_files_to_pruneyard() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create files with same content in both repositories
    fs::write(source_dir.path().join("common.txt"), "shared content").unwrap();
    fs::write(local_dir.path().join("common.txt"), "shared content").unwrap();
    fs::write(local_dir.path().join("unique.txt"), "unique content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 1 file(s)"));
    assert!(stdout.contains("common.txt"));
    assert!(stdout.contains("1 duplicates"));
    
    // Verify common.txt was moved to pruneyard
    assert!(!local_dir.path().join("common.txt").exists());
    assert!(local_dir.path().join(".oci/pruneyard/common.txt").exists());
    
    // Verify unique.txt still exists
    assert!(local_dir.path().join("unique.txt").exists());
    
    // Verify index was updated
    let (stdout, _, _) = run_oci(&["ls", "-r"], local_dir.path());
    assert!(!stdout.contains("common.txt"));
    assert!(stdout.contains("unique.txt"));
}

#[test]
fn test_prune_fails_with_pending_changes() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create and index a file in local
    fs::write(local_dir.path().join("test.txt"), "content").unwrap();
    run_oci(&["update"], local_dir.path());
    
    // Modify the file without updating the index
    fs::write(local_dir.path().join("test.txt"), "modified").unwrap();
    
    // Prune should fail due to pending changes
    let source_path = source_dir.path().to_str().unwrap();
    let (_, stderr, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("pending changes"));
}

#[test]
fn test_prune_fails_with_source_pending_changes() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();

    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());

    // Create and index a file in source
    fs::write(source_dir.path().join("test.txt"), "content").unwrap();
    run_oci(&["update"], source_dir.path());

    // Create and index a file in local
    fs::write(local_dir.path().join("local.txt"), "local content").unwrap();
    run_oci(&["update"], local_dir.path());

    // Modify the source file without updating the index
    fs::write(source_dir.path().join("test.txt"), "modified").unwrap();

    // Prune should fail due to pending changes in source
    let source_path = source_dir.path().to_str().unwrap();
    let (_, stderr, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("pending changes"));
    assert!(stderr.contains("source index"));
}

#[test]
fn test_prune_purge_deletes_files() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create files with same content
    fs::write(source_dir.path().join("file.txt"), "content").unwrap();
    fs::write(local_dir.path().join("file.txt"), "content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());
    
    // Verify file is in pruneyard
    assert!(local_dir.path().join(".oci/pruneyard/file.txt").exists());
    
    // Purge pruned files
    let (stdout, _, exit_code) = run_oci(&["prune", "--purge", "--force"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Permanently deleted 1 pruned file(s)"));
    
    // Verify pruneyard is gone
    assert!(!local_dir.path().join(".oci/pruneyard").exists());
}

#[test]
fn test_prune_purge_fails_with_pending_changes() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create files with same content
    fs::write(source_dir.path().join("file.txt"), "content").unwrap();
    fs::write(local_dir.path().join("file.txt"), "content").unwrap();
    fs::write(local_dir.path().join("other.txt"), "other content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());
    
    // Verify file is in pruneyard
    assert!(local_dir.path().join(".oci/pruneyard/file.txt").exists());
    
    // Modify a file without updating the index
    fs::write(local_dir.path().join("other.txt"), "modified").unwrap();
    
    // Purge should fail due to pending changes
    let (_, stderr, exit_code) = run_oci(&["prune", "--purge", "--force"], local_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("pending changes"));
    assert!(stderr.contains("local index"));
}

#[test]
fn test_prune_without_source_fails() {
    let local_dir = TempDir::new().unwrap();
    run_oci(&["init"], local_dir.path());
    
    // Prune without source path should fail
    let (_, stderr, exit_code) = run_oci(&["prune"], local_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("Source path is required"));
}

#[test]
fn test_prune_same_index_fails() {
    let local_dir = TempDir::new().unwrap();
    run_oci(&["init"], local_dir.path());
    
    // Prune using the same index as source should fail
    let source_path = local_dir.path().to_str().unwrap();
    let (_, stderr, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("Cannot prune using the same index"));
}

#[test]
fn test_prune_restore() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create files with same content in both repositories
    fs::write(source_dir.path().join("common.txt"), "shared content").unwrap();
    fs::write(local_dir.path().join("common.txt"), "shared content").unwrap();
    fs::write(local_dir.path().join("unique.txt"), "unique content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());
    
    // Verify file was pruned
    assert!(!local_dir.path().join("common.txt").exists());
    assert!(local_dir.path().join(".oci/pruneyard/common.txt").exists());
    
    // Restore pruned files
    let (stdout, _, exit_code) = run_oci(&["prune", "--restore"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Restored 1 file(s)"));
    assert!(stdout.contains("common.txt"));
    
    // Verify file was restored
    assert!(local_dir.path().join("common.txt").exists());
    assert!(!local_dir.path().join(".oci/pruneyard").exists());
    
    // Verify file is back in index
    let (stdout, _, _) = run_oci(&["ls", "-r"], local_dir.path());
    assert!(stdout.contains("common.txt"));
    assert!(stdout.contains("unique.txt"));
    
    // Verify content is correct
    let content = fs::read_to_string(local_dir.path().join("common.txt")).unwrap();
    assert_eq!(content, "shared content");
}

#[test]
fn test_prune_restore_preserves_directory_structure() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create nested directory structure
    fs::create_dir_all(source_dir.path().join("subdir/nested")).unwrap();
    fs::create_dir_all(local_dir.path().join("subdir/nested")).unwrap();
    
    // Create files with same content in nested directories
    fs::write(source_dir.path().join("subdir/nested/file.txt"), "content").unwrap();
    fs::write(local_dir.path().join("subdir/nested/file.txt"), "content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());
    
    // Verify file was pruned
    assert!(!local_dir.path().join("subdir/nested/file.txt").exists());
    
    // Restore pruned files
    let (stdout, _, exit_code) = run_oci(&["prune", "--restore"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Restored 1 file(s)"));
    
    // Verify file was restored with correct directory structure
    assert!(local_dir.path().join("subdir/nested/file.txt").exists());
    
    // Verify content is correct
    let content = fs::read_to_string(local_dir.path().join("subdir/nested/file.txt")).unwrap();
    assert_eq!(content, "content");
}

#[test]
fn test_prune_preserves_directory_structure() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create nested directory structure
    fs::create_dir_all(source_dir.path().join("subdir/nested")).unwrap();
    fs::create_dir_all(local_dir.path().join("subdir/nested")).unwrap();
    
    // Create files with same content in nested directories
    fs::write(source_dir.path().join("subdir/nested/file.txt"), "content").unwrap();
    fs::write(local_dir.path().join("subdir/nested/file.txt"), "content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source
    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 1 file(s)"));
    assert!(stdout.contains("1 duplicates"));
    
    // Verify file was moved with directory structure preserved
    assert!(!local_dir.path().join("subdir/nested/file.txt").exists());
    assert!(local_dir.path().join(".oci/pruneyard/subdir/nested/file.txt").exists());
}

#[test]
fn test_prune_removes_ignored_files() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Add ignore pattern to source
    run_oci(&["ignore", "*.log"], source_dir.path());
    
    // Create files in local (including a .log file)
    fs::write(local_dir.path().join("important.txt"), "keep this").unwrap();
    fs::write(local_dir.path().join("debug.log"), "remove this").unwrap();
    
    // Update local index
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source - should remove .log file based on source ignore patterns
    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 1 file(s)"));
    assert!(stdout.contains("debug.log"));
    assert!(stdout.contains("ignored"));
    assert!(stdout.contains("1 ignored"));
    
    // Verify .log file was pruned
    assert!(!local_dir.path().join("debug.log").exists());
    assert!(local_dir.path().join(".oci/pruneyard/debug.log").exists());
    
    // Verify important.txt still exists
    assert!(local_dir.path().join("important.txt").exists());
}

#[test]
fn test_prune_no_ignore_flag() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Add ignore pattern to source
    run_oci(&["ignore", "*.log"], source_dir.path());
    
    // Create files in local (including a .log file)
    fs::write(local_dir.path().join("important.txt"), "keep this").unwrap();
    fs::write(local_dir.path().join("debug.log"), "keep this too").unwrap();
    
    // Update local index
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source with --no-ignore flag
    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path, "--no-ignore"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No files to prune") || stdout.contains("Pruned 0 file(s)"));
    
    // Verify .log file was NOT pruned
    assert!(local_dir.path().join("debug.log").exists());
    assert!(local_dir.path().join("important.txt").exists());
}

#[test]
fn test_prune_removes_empty_directories() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Create nested directory structure with a file
    fs::create_dir_all(local_dir.path().join("dir1/dir2/dir3")).unwrap();
    fs::write(local_dir.path().join("dir1/dir2/dir3/file.txt"), "content").unwrap();
    
    // Create an already-empty directory
    fs::create_dir_all(local_dir.path().join("empty1/empty2")).unwrap();
    
    // Create a file with same content in source
    fs::write(source_dir.path().join("file.txt"), "content").unwrap();
    
    // Update both indices
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());
    
    // Verify directory structures exist before prune
    assert!(local_dir.path().join("dir1/dir2/dir3").exists());
    assert!(local_dir.path().join("empty1/empty2").exists());
    
    // Prune local using source (will remove the file)
    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());
    
    // Verify file is gone
    assert!(!local_dir.path().join("dir1/dir2/dir3/file.txt").exists());
    
    // Verify all empty directories were removed
    assert!(!local_dir.path().join("dir1/dir2/dir3").exists());
    assert!(!local_dir.path().join("dir1/dir2").exists());
    assert!(!local_dir.path().join("dir1").exists());
    assert!(!local_dir.path().join("empty1/empty2").exists());
    assert!(!local_dir.path().join("empty1").exists());
}

#[test]
fn test_duplicates_finds_duplicate_files() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create files with duplicate content
    fs::write(test_dir.path().join("file1.txt"), "content").unwrap();
    fs::write(test_dir.path().join("file2.txt"), "content").unwrap();
    fs::write(test_dir.path().join("file3.txt"), "different").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    // Run duplicates command
    let (stdout, _, exit_code) = run_oci(&["duplicates"], test_dir.path());
    assert_eq!(exit_code, 0);
    
    // Should find the duplicates
    assert!(stdout.contains("Found 2 duplicate file(s) in 1 group(s)"));
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(!stdout.contains("file3.txt")); // This one is unique
}

#[test]
fn test_duplicates_no_duplicates() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create files with unique content
    fs::write(test_dir.path().join("file1.txt"), "content1").unwrap();
    fs::write(test_dir.path().join("file2.txt"), "content2").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    // Run duplicates command
    let (stdout, _, exit_code) = run_oci(&["duplicates"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No duplicate files found"));
}

#[test]
fn test_duplicates_multiple_groups() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create two groups of duplicates
    fs::write(test_dir.path().join("file1.txt"), "content_a").unwrap();
    fs::write(test_dir.path().join("file2.txt"), "content_a").unwrap();
    fs::write(test_dir.path().join("file3.txt"), "content_b").unwrap();
    fs::write(test_dir.path().join("file4.txt"), "content_b").unwrap();
    fs::write(test_dir.path().join("file5.txt"), "unique").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    // Run duplicates command
    let (stdout, _, exit_code) = run_oci(&["duplicates"], test_dir.path());
    assert_eq!(exit_code, 0);
    
    // Should find two groups with 4 total duplicates
    assert!(stdout.contains("Found 4 duplicate file(s) in 2 group(s)"));
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(stdout.contains("file3.txt"));
    assert!(stdout.contains("file4.txt"));
    assert!(!stdout.contains("file5.txt")); // This one is unique
}

#[test]
fn test_duplicates_recursive() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create files in subdirectories with duplicate content
    fs::create_dir_all(test_dir.path().join("subdir")).unwrap();
    fs::write(test_dir.path().join("file1.txt"), "content").unwrap();
    fs::write(test_dir.path().join("subdir/file2.txt"), "content").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    // Run duplicates (always recursive - should find all duplicates across subdirectories)
    let (stdout, _, exit_code) = run_oci(&["duplicates"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Found 2 duplicate file(s) in 1 group(s)"));
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
}

#[test]
fn test_stats_empty_index() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    let (stdout, _, exit_code) = run_oci(&["stats"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Index is empty"));
}

#[test]
fn test_stats_with_files() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create some test files
    fs::write(test_dir.path().join("file1.txt"), "hello world").unwrap();
    fs::write(test_dir.path().join("file2.txt"), "hello world").unwrap(); // duplicate
    fs::write(test_dir.path().join("file3.txt"), "different content").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    let (stdout, _, exit_code) = run_oci(&["stats"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Index Statistics:"));
    assert!(stdout.contains("Total files: 3"));
    assert!(stdout.contains("Unique hashes: 2"));
    assert!(stdout.contains("Duplicate files: 2")); // 2 files with same content
    assert!(stdout.contains("Duplicate groups: 1"));
    assert!(stdout.contains("Wasted space:"));
    assert!(stdout.contains("Storage efficiency:"));
}

#[test]
fn test_stats_no_duplicates() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create unique files
    fs::write(test_dir.path().join("file1.txt"), "content1").unwrap();
    fs::write(test_dir.path().join("file2.txt"), "content2").unwrap();
    fs::write(test_dir.path().join("file3.txt"), "content3").unwrap();
    
    run_oci(&["update"], test_dir.path());
    
    let (stdout, _, exit_code) = run_oci(&["stats"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Total files: 3"));
    assert!(stdout.contains("Unique hashes: 3"));
    assert!(stdout.contains("Duplicate files: 0"));
    assert!(!stdout.contains("Duplicate groups:")); // Should not show when there are no duplicates
    assert!(!stdout.contains("Wasted space:")); // Should not show when there are no duplicates
    assert!(stdout.contains("Storage efficiency: 100.00%"));
}

#[test]
fn test_status_dot_from_subdirectory_with_spaces() {
    // Regression test for bug where "oci status ." from a subdirectory with spaces
    // would show files as both added (+) and deleted (-)
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create a subdirectory with spaces in the name
    let subdir = test_dir.path().join("Google Drive").join("Papers");
    fs::create_dir_all(&subdir).unwrap();
    
    // Create files in the subdirectory
    fs::write(subdir.join("paper1.pdf"), "content1").unwrap();
    fs::write(subdir.join("paper2.pdf"), "content2").unwrap();
    
    // Update the index
    run_oci(&["update"], test_dir.path());
    
    // Run "status ." from the subdirectory
    let (stdout, _, exit_code) = run_oci(&["status", "."], &subdir);
    assert_eq!(exit_code, 0);
    
    // Should show "No changes"
    assert!(stdout.contains("No changes"), "Expected 'No changes' but got:\n{}", stdout);
    
    // Should NOT show both + and - for the same file (the bug we're testing for)
    assert!(!stdout.contains("+ "), "Unexpectedly found '+' (added) in output:\n{}", stdout);
    assert!(!stdout.contains("- "), "Unexpectedly found '-' (deleted) in output:\n{}", stdout);
    
    // Modify one file and verify status detects it correctly
    fs::write(subdir.join("paper1.pdf"), "modified content").unwrap();
    let (stdout, _, exit_code) = run_oci(&["status", "."], &subdir);
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("U "), "Should show 'U' for modified file");
    assert!(stdout.contains("paper1.pdf"));
    // Should show exactly one line with U, not both + and -
    let u_count = stdout.matches("U ").count();
    let plus_count = stdout.matches("+ ").count();
    let minus_count = stdout.matches("- ").count();
    assert_eq!(u_count, 1, "Should have exactly 1 'U' line");
    assert_eq!(plus_count, 0, "Should have 0 '+' lines");
    assert_eq!(minus_count, 0, "Should have 0 '-' lines");
}

#[test]
fn test_update_dot_from_subdirectory_with_spaces() {
    // Regression test to ensure "oci update ." works correctly from subdirectories with spaces
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create a subdirectory with spaces
    let subdir = test_dir.path().join("My Documents").join("Projects");
    fs::create_dir_all(&subdir).unwrap();
    
    // Create files in the subdirectory
    fs::write(subdir.join("file1.txt"), "content1").unwrap();
    fs::write(subdir.join("file2.txt"), "content2").unwrap();
    
    // Update the index from the subdirectory using "."
    let (stdout, _, exit_code) = run_oci(&["update", "."], &subdir);
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 2 file(s)"));
    assert!(stdout.contains("2 added"));
    
    // Verify status shows no changes
    let (stdout, _, exit_code) = run_oci(&["status", "."], &subdir);
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No changes"));
    
    // Modify a file and update again
    fs::write(subdir.join("file1.txt"), "modified").unwrap();
    let (stdout, _, exit_code) = run_oci(&["update", "."], &subdir);
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Updated 1 file(s)"));
    assert!(stdout.contains("1 updated"));
    
    // Verify status shows no changes after update
    let (stdout, _, exit_code) = run_oci(&["status", "."], &subdir);
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No changes"));
}

#[test]
fn test_prune_ignored_flag_with_source() {
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();
    
    // Initialize both repositories
    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());
    
    // Add ignore pattern to local ignore (not source)
    run_oci(&["ignore", "*.tmp"], local_dir.path());
    
    // Create files in source
    fs::write(source_dir.path().join("shared.txt"), "shared content").unwrap();
    run_oci(&["update"], source_dir.path());
    
    // Create files in local (including a .tmp file and a duplicate)
    fs::write(local_dir.path().join("shared.txt"), "shared content").unwrap();
    fs::write(local_dir.path().join("temp.tmp"), "temporary file").unwrap();
    fs::write(local_dir.path().join("unique.txt"), "keep this").unwrap();
    run_oci(&["update"], local_dir.path());
    
    // Prune local using source with --ignored flag
    // Should remove both the duplicate and the locally ignored file
    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path, "--ignored"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 2 file(s)"));
    assert!(stdout.contains("shared.txt"));
    assert!(stdout.contains("temp.tmp"));
    assert!(stdout.contains("1 duplicates, 1 ignored"));
    
    // Verify files were pruned
    assert!(!local_dir.path().join("shared.txt").exists());
    assert!(!local_dir.path().join("temp.tmp").exists());
    assert!(local_dir.path().join(".oci/pruneyard/shared.txt").exists());
    assert!(local_dir.path().join(".oci/pruneyard/temp.tmp").exists());
    
    // Verify unique.txt still exists
    assert!(local_dir.path().join("unique.txt").exists());
}

#[test]
fn test_prune_ignored_flag_without_source() {
    let local_dir = TempDir::new().unwrap();
    
    // Initialize repository
    run_oci(&["init"], local_dir.path());
    
    // Add ignore patterns to local ignore
    run_oci(&["ignore", "*.log"], local_dir.path());
    run_oci(&["ignore", "*.tmp"], local_dir.path());
    
    // Create files (including ignored files)
    fs::write(local_dir.path().join("important.txt"), "keep this").unwrap();
    fs::write(local_dir.path().join("debug.log"), "remove this").unwrap();
    fs::write(local_dir.path().join("cache.tmp"), "remove this too").unwrap();
    
    // Update index - ignored files won't be added to index
    run_oci(&["update"], local_dir.path());
    
    // Verify only important.txt is in the index
    let (stdout, _, _) = run_oci(&["ls"], local_dir.path());
    assert!(stdout.contains("important.txt"));
    assert!(!stdout.contains("debug.log"));
    assert!(!stdout.contains("cache.tmp"));
    
    // Prune using --ignored flag without source
    let (stdout, _, exit_code) = run_oci(&["prune", "--ignored"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 2 ignored file(s)"));
    assert!(stdout.contains("debug.log"));
    assert!(stdout.contains("cache.tmp"));
    
    // Verify ignored files were pruned
    assert!(!local_dir.path().join("debug.log").exists());
    assert!(!local_dir.path().join("cache.tmp").exists());
    assert!(local_dir.path().join(".oci/pruneyard/debug.log").exists());
    assert!(local_dir.path().join(".oci/pruneyard/cache.tmp").exists());
    
    // Verify important.txt still exists
    assert!(local_dir.path().join("important.txt").exists());
}

#[test]
fn test_prune_ignored_flag_with_indexed_ignored_files() {
    let local_dir = TempDir::new().unwrap();
    
    // Initialize repository
    run_oci(&["init"], local_dir.path());
    
    // Create files and add them to index
    fs::write(local_dir.path().join("important.txt"), "keep this").unwrap();
    fs::write(local_dir.path().join("old_cache.tmp"), "was not ignored initially").unwrap();
    run_oci(&["update"], local_dir.path());
    
    // Now add ignore pattern for .tmp files and create a new ignored file
    run_oci(&["ignore", "*.tmp"], local_dir.path());
    
    // The old .tmp file is still in the index (but now matches ignore patterns)
    // Run update to sync the index with current ignore patterns
    run_oci(&["update"], local_dir.path());
    
    // The .tmp file should be removed from index by update (since it's now ignored)
    let (stdout, _, _) = run_oci(&["ls"], local_dir.path());
    assert!(!stdout.contains("old_cache.tmp"), "Ignored file should be removed from index by update");
    
    // But the file still exists on filesystem
    assert!(local_dir.path().join("old_cache.tmp").exists());
    
    // Prune using --ignored flag - should move ignored files from filesystem to pruneyard
    let (stdout, stderr, exit_code) = run_oci(&["prune", "--ignored"], local_dir.path());
    if exit_code != 0 {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Pruned 1 ignored file(s)"));
    assert!(stdout.contains("old_cache.tmp"));
    
    // Verify file was pruned from filesystem
    assert!(!local_dir.path().join("old_cache.tmp").exists());
    assert!(local_dir.path().join(".oci/pruneyard/old_cache.tmp").exists());
    
    // Verify important.txt still exists
    assert!(local_dir.path().join("important.txt").exists());
}

#[test]
fn test_prune_restore_skips_existing_destination() {
    // Regression: prune --restore must not silently overwrite a file the user
    // has placed at the original location since pruning. The pruned copy must
    // remain in the pruneyard so the user can resolve the conflict.
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();

    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());

    // Create matching file in both repos and prune the local copy
    fs::write(source_dir.path().join("common.txt"), "shared").unwrap();
    fs::write(local_dir.path().join("common.txt"), "shared").unwrap();
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());

    let source_path = source_dir.path().to_str().unwrap();
    run_oci(&["prune", source_path], local_dir.path());

    // After prune: file is in pruneyard, not on disk
    assert!(!local_dir.path().join("common.txt").exists());
    assert!(local_dir.path().join(".oci/pruneyard/common.txt").exists());

    // User creates a NEW file at the original location
    fs::write(local_dir.path().join("common.txt"), "user's new content").unwrap();

    // Restore must NOT overwrite the user's file
    let (stdout, stderr, exit_code) = run_oci(&["prune", "--restore"], local_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        stderr.contains("Skipping") && stderr.contains("common.txt"),
        "Expected skip warning on stderr, got:\n{}",
        stderr
    );
    assert!(stdout.contains("Restored 0 file(s)"));
    assert!(stdout.contains("Skipped 1 file(s)"));

    // The user's file is intact
    let content = fs::read_to_string(local_dir.path().join("common.txt")).unwrap();
    assert_eq!(content, "user's new content");

    // The pruned copy stays in the pruneyard for manual resolution
    assert!(local_dir.path().join(".oci/pruneyard/common.txt").exists());
    let pruned_content =
        fs::read_to_string(local_dir.path().join(".oci/pruneyard/common.txt")).unwrap();
    assert_eq!(pruned_content, "shared");
}

#[test]
fn test_dot_oci_prefix_does_not_match_unrelated_dotfiles() {
    // Regression: files/dirs whose name starts with ".oci" (e.g. ".ocirc",
    // ".oci-backup") must NOT be treated as the .oci index directory and
    // silently excluded from scans.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    // File whose name starts with ".oci" but is not the index directory
    fs::write(test_dir.path().join(".ocirc"), "config").unwrap();
    // Directory whose name starts with ".oci"
    fs::create_dir(test_dir.path().join(".oci-backup")).unwrap();
    fs::write(test_dir.path().join(".oci-backup/data.txt"), "backup").unwrap();
    // Regular file as a control
    fs::write(test_dir.path().join("regular.txt"), "regular").unwrap();

    let (stdout, _, exit_code) = run_oci(&["update"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("3 added"), "Expected 3 added, got:\n{}", stdout);

    let (stdout, _, _) = run_oci(&["ls", "-r"], test_dir.path());
    assert!(stdout.contains(".ocirc"), ".ocirc should be indexed:\n{}", stdout);
    assert!(
        stdout.contains(".oci-backup/data.txt"),
        ".oci-backup/data.txt should be indexed:\n{}",
        stdout
    );
    assert!(stdout.contains("regular.txt"));
}

#[test]
fn test_reset_interactive_yes_clears_index() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    fs::write(test_dir.path().join("a.txt"), "x").unwrap();
    run_oci(&["update"], test_dir.path());

    let (stdout, _, exit_code) = run_oci_with_stdin(&["reset"], test_dir.path(), "y\n");
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Reset index"));

    let (ls_stdout, _, _) = run_oci(&["ls"], test_dir.path());
    assert!(
        !ls_stdout.contains("a.txt"),
        "Expected index cleared, got ls:\n{}",
        ls_stdout
    );
}

#[test]
fn test_reset_interactive_no_cancels() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    fs::write(test_dir.path().join("a.txt"), "x").unwrap();
    run_oci(&["update"], test_dir.path());

    let (stdout, _, exit_code) = run_oci_with_stdin(&["reset"], test_dir.path(), "n\n");
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Reset cancelled"));

    // Index should still contain a.txt
    let (ls_stdout, _, _) = run_oci(&["ls"], test_dir.path());
    assert!(ls_stdout.contains("a.txt"));
}

#[test]
fn test_deinit_interactive_yes_removes_oci_dir() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    let (stdout, _, exit_code) = run_oci_with_stdin(&["deinit"], test_dir.path(), "y\n");
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Deinitialized"));
    assert!(!test_dir.path().join(".oci").exists());
}

#[test]
fn test_deinit_interactive_no_cancels() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    let (stdout, _, exit_code) = run_oci_with_stdin(&["deinit"], test_dir.path(), "n\n");
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Deinit cancelled"));
    assert!(test_dir.path().join(".oci").exists());
}

#[test]
fn test_update_single_file_add_and_modify() {
    // The `update_single_file` path: passing a specific file path to update
    // adds it on first run and reports it as updated on content change. Sibling
    // files must not be touched by a single-file update.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    fs::write(test_dir.path().join("target.txt"), "v1").unwrap();
    fs::write(test_dir.path().join("sibling.txt"), "untouched").unwrap();

    // First single-file update: target.txt added, sibling untouched
    let (stdout, _, exit_code) = run_oci(&["update", "target.txt"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("1 added"), "Expected 1 added, got:\n{}", stdout);
    assert!(
        stdout.lines().any(|l| l.starts_with("+") && l.ends_with("target.txt")),
        "Expected '+' marker for target.txt, got:\n{}",
        stdout
    );

    let (ls_stdout, _, _) = run_oci(&["ls"], test_dir.path());
    assert!(ls_stdout.contains("target.txt"));
    assert!(
        !ls_stdout.contains("sibling.txt"),
        "Single-file update must not index siblings, got:\n{}",
        ls_stdout
    );

    // Modify content; sleep briefly to ensure mtime advances
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(test_dir.path().join("target.txt"), "v2 longer").unwrap();
    let (stdout, _, exit_code) = run_oci(&["update", "target.txt"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        stdout.contains("1 updated"),
        "Expected 1 updated, got:\n{}",
        stdout
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("U") && l.ends_with("target.txt")),
        "Expected 'U' marker for target.txt, got:\n{}",
        stdout
    );
}

#[test]
fn test_grep_with_no_match() {
    // `oci grep <hash>` for a hash that doesn't exist must report the
    // "No files found" message and exit 0 (not crash).
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    fs::write(test_dir.path().join("a.txt"), "hello").unwrap();
    run_oci(&["update"], test_dir.path());

    let nonexistent_hash = "0".repeat(64);
    let (stdout, _, exit_code) = run_oci(&["grep", &nonexistent_hash], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        stdout.contains("No files found with hash:"),
        "Expected 'no files found' message, got:\n{}",
        stdout
    );
}

#[test]
fn test_status_verbose_shows_unchanged_and_ignored_markers() {
    // README documents `=` (unchanged) and `I` (ignored) markers shown only
    // with `-v`. Confirm both appear with `-v` and neither without.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    fs::write(test_dir.path().join("kept.txt"), "x").unwrap();
    fs::write(test_dir.path().join("noise.log"), "y").unwrap();
    run_oci(&["ignore", "*.log"], test_dir.path());
    run_oci(&["update"], test_dir.path());

    fn has_marker_line(stdout: &str, marker: &str, filename: &str) -> bool {
        stdout.lines().any(|line| {
            line.starts_with(marker) && line.ends_with(filename)
        })
    }

    // Without -v: neither marker should be present
    let (stdout_plain, _, _) = run_oci(&["status"], test_dir.path());
    assert!(!has_marker_line(&stdout_plain, "=", "kept.txt"));
    assert!(!has_marker_line(&stdout_plain, "I", "noise.log"));

    // With -v: both markers should appear
    let (stdout_v, _, exit_code) = run_oci(&["status", "-v"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        has_marker_line(&stdout_v, "=", "kept.txt"),
        "Expected '=' marker line for kept.txt in status -v output:\n{}",
        stdout_v
    );
    assert!(
        has_marker_line(&stdout_v, "I", "noise.log"),
        "Expected 'I' marker line for noise.log in status -v output:\n{}",
        stdout_v
    );
}

#[test]
fn test_update_verbose_shows_unchanged_and_ignored_markers() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    fs::write(test_dir.path().join("kept.txt"), "x").unwrap();
    fs::write(test_dir.path().join("noise.log"), "y").unwrap();
    run_oci(&["ignore", "*.log"], test_dir.path());
    run_oci(&["update"], test_dir.path());

    fn has_marker_line(stdout: &str, marker: &str, filename: &str) -> bool {
        stdout.lines().any(|line| {
            line.starts_with(marker) && line.ends_with(filename)
        })
    }

    // Without -v: a no-op update should not display unchanged/ignored
    let (stdout_plain, _, _) = run_oci(&["update"], test_dir.path());
    assert!(!has_marker_line(&stdout_plain, "=", "kept.txt"));
    assert!(!has_marker_line(&stdout_plain, "I", "noise.log"));

    let (stdout_v, _, exit_code) = run_oci(&["update", "-v"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        has_marker_line(&stdout_v, "=", "kept.txt"),
        "Expected '=' marker line for kept.txt in update -v output:\n{}",
        stdout_v
    );
    assert!(
        has_marker_line(&stdout_v, "I", "noise.log"),
        "Expected 'I' marker line for noise.log in update -v output:\n{}",
        stdout_v
    );
}

#[test]
fn test_ocignore_to_ignore_migration() {
    // Repos created with older versions stored patterns in .oci/ocignore.
    // Loading patterns must transparently rename it to .oci/ignore.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    // Replace the modern ignore with an old-style ocignore
    let oci_dir = test_dir.path().join(".oci");
    let ignore_path = oci_dir.join("ignore");
    let old_path = oci_dir.join("ocignore");
    fs::remove_file(&ignore_path).unwrap();
    fs::write(&old_path, "*.legacy\n").unwrap();

    fs::write(test_dir.path().join("data.legacy"), "x").unwrap();
    fs::write(test_dir.path().join("keep.txt"), "y").unwrap();

    let (stdout, _, exit_code) = run_oci(&["update"], test_dir.path());
    assert_eq!(exit_code, 0);
    // The legacy file should be ignored; only keep.txt is added
    assert!(
        stdout.contains("1 added"),
        "Expected only keep.txt to be added, got:\n{}",
        stdout
    );

    // ocignore should have been renamed to ignore
    assert!(ignore_path.exists(), "Migration should produce .oci/ignore");
    assert!(!old_path.exists(), "Migration should remove .oci/ocignore");

    let migrated = fs::read_to_string(&ignore_path).unwrap();
    assert!(migrated.contains("*.legacy"));
}

#[test]
fn test_ignore_no_argument_uses_current_directory() {
    // `oci ignore` with no argument should add the current directory
    // (relative to the repo root) as the ignored pattern.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    let sub = test_dir.path().join("logs");
    fs::create_dir(&sub).unwrap();

    let (stdout, _, exit_code) = run_oci(&["ignore"], &sub);
    assert_eq!(exit_code, 0);
    assert!(
        stdout.contains("Added pattern to ignore: logs"),
        "Expected current dir relative-to-root to be added, got:\n{}",
        stdout
    );

    let ignore_contents = fs::read_to_string(test_dir.path().join(".oci/ignore")).unwrap();
    assert!(
        ignore_contents.lines().any(|l| l.trim() == "logs"),
        "Expected 'logs' line in ignore file:\n{}",
        ignore_contents
    );
}

#[test]
fn test_version_mismatch_warning() {
    // A .oci/config with an older version should produce a stderr warning on
    // any subsequent command, but the command itself must still succeed.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    let config_path = test_dir.path().join(".oci/config");
    fs::write(&config_path, "version=0.0.1\n").unwrap();

    let (_, stderr, exit_code) = run_oci(&["ls"], test_dir.path());
    assert_eq!(exit_code, 0, "Command must still succeed despite warning");
    assert!(
        stderr.contains("Warning: Index version mismatch"),
        "Expected version-mismatch warning, got stderr:\n{}",
        stderr
    );
    assert!(stderr.contains("0.0.1"), "Should mention the stored version");
}

#[test]
fn test_update_single_file_detects_deletion() {
    // Regression: `oci update path/to/file.txt` on a file that has been deleted
    // from disk but is still in the index must detect the deletion and update
    // the index, matching the directory-update path. Previously it errored with
    // "Path does not exist" and left the index untouched.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    fs::write(test_dir.path().join("doomed.txt"), "bye").unwrap();
    run_oci(&["update"], test_dir.path());

    // Confirm it's indexed
    let (ls_stdout, _, _) = run_oci(&["ls"], test_dir.path());
    assert!(ls_stdout.contains("doomed.txt"));

    // Delete from disk, then ask oci to update that specific path
    fs::remove_file(test_dir.path().join("doomed.txt")).unwrap();

    let (stdout, _, exit_code) = run_oci(&["update", "doomed.txt"], test_dir.path());
    assert_eq!(exit_code, 0, "Expected success, got:\n{}", stdout);
    assert!(
        stdout.contains("- doomed.txt") || stdout.contains("0 added, 0 updated, 1 removed"),
        "Expected deletion to be reported, got:\n{}",
        stdout
    );

    // It should no longer be in the index
    let (ls_stdout, _, _) = run_oci(&["ls"], test_dir.path());
    assert!(
        !ls_stdout.contains("doomed.txt"),
        "Expected doomed.txt to be removed from index, got ls output:\n{}",
        ls_stdout
    );
}

#[test]
fn test_prune_reason_duplicate_wins_over_ignored() {
    // Regression: a file that is both a duplicate AND matches an ignore pattern
    // must be counted as a duplicate, not silently reclassified as "ignored".
    // To trigger both conditions without making source itself have pending
    // changes, store the same content under a non-matching name in source.
    let source_dir = TempDir::new().unwrap();
    let local_dir = TempDir::new().unwrap();

    run_oci(&["init"], source_dir.path());
    run_oci(&["init"], local_dir.path());

    // Same content, different paths — source's index will have the hash
    fs::write(source_dir.path().join("other.txt"), "content").unwrap();
    fs::write(local_dir.path().join("shared.log"), "content").unwrap();
    run_oci(&["update"], source_dir.path());
    run_oci(&["update"], local_dir.path());

    // Source ignores *.log — doesn't affect source's other.txt, but the
    // local shared.log matches BOTH the hash AND this pattern.
    run_oci(&["ignore", "*.log"], source_dir.path());

    let source_path = source_dir.path().to_str().unwrap();
    let (stdout, _, exit_code) = run_oci(&["prune", source_path], local_dir.path());
    assert_eq!(exit_code, 0, "prune failed:\n{}", stdout);
    assert!(
        stdout.contains("1 duplicates, 0 ignored"),
        "Expected duplicate to win over ignored, got:\n{}",
        stdout
    );
}

#[test]
fn test_ignore_add_dedupes() {
    // Regression: running `oci ignore <pattern>` twice must not append the
    // pattern twice; the ignore file should not grow unbounded.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    let (stdout1, _, exit_code) = run_oci(&["ignore", "*.log"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout1.contains("Added pattern to ignore: *.log"));

    let (stdout2, _, exit_code) = run_oci(&["ignore", "*.log"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(
        stdout2.contains("already"),
        "Expected dedupe message, got:\n{}",
        stdout2
    );

    let ignore_contents = fs::read_to_string(test_dir.path().join(".oci/ignore")).unwrap();
    let count = ignore_contents
        .lines()
        .filter(|l| l.trim() == "*.log")
        .count();
    assert_eq!(count, 1, "Expected exactly one *.log line, got {}:\n{}", count, ignore_contents);
}

#[test]
fn test_config_missing_version_field_errors() {
    // Regression: a config file that exists but has no version= line must
    // surface an error, not silently masquerade as the current tool version.
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());

    // Corrupt the config: replace with content that has no version= line
    let config_path = test_dir.path().join(".oci/config");
    fs::write(&config_path, "# comment only, no version\n").unwrap();

    let (_, stderr, exit_code) = run_oci(&["ls"], test_dir.path());
    assert_ne!(exit_code, 0, "Expected non-zero exit, got stderr:\n{}", stderr);
    assert!(
        stderr.contains("missing required 'version' field"),
        "Expected corruption error on stderr, got:\n{}",
        stderr
    );
}

#[test]
fn test_hogs_empty_index() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    let (stdout, _, exit_code) = run_oci(&["hogs"], test_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("No files in index"));
}

#[test]
fn test_hogs_sorts_by_size() {
    let test_dir = TempDir::new().unwrap();
    run_oci(&["init"], test_dir.path());
    
    // Create files of different sizes
    fs::write(test_dir.path().join("small.txt"), "x").unwrap(); // 1 byte
    fs::write(test_dir.path().join("medium.txt"), "x".repeat(100)).unwrap(); // 100 bytes
    fs::write(test_dir.path().join("large.txt"), "x".repeat(1000)).unwrap(); // 1000 bytes
    
    run_oci(&["update"], test_dir.path());
    
    let (stdout, _, exit_code) = run_oci(&["hogs"], test_dir.path());
    assert_eq!(exit_code, 0);
    
    // Verify all files are listed
    assert!(stdout.contains("small.txt"));
    assert!(stdout.contains("medium.txt"));
    assert!(stdout.contains("large.txt"));
    
    // Verify they appear in size order (largest first)
    let large_pos = stdout.find("large.txt").unwrap();
    let medium_pos = stdout.find("medium.txt").unwrap();
    let small_pos = stdout.find("small.txt").unwrap();
    
    assert!(large_pos < medium_pos, "large.txt should appear before medium.txt");
    assert!(medium_pos < small_pos, "medium.txt should appear before small.txt");
}
