use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;
use tempfile::TempDir;

static INIT: Once = Once::new();
static mut OCI_BIN: Option<PathBuf> = None;

fn get_oci_binary() -> &'static Path {
    unsafe {
        INIT.call_once(|| {
            // Build the binary once
            let output = Command::new("cargo")
                .args(&["build", "--quiet"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .expect("Failed to build oci");
            
            if !output.status.success() {
                panic!("Failed to build oci binary");
            }
            
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let bin_path = manifest_dir.join("target/debug/oci");
            OCI_BIN = Some(bin_path);
        });
        
        OCI_BIN.as_ref().unwrap()
    }
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
fn test_init_creates_ocignore_with_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let (_, _, exit_code) = run_oci(&["init"], temp_dir.path());
    
    assert_eq!(exit_code, 0);
    
    // Verify .ocignore file exists
    let ocignore_path = temp_dir.path().join(".oci/.ocignore");
    assert!(ocignore_path.exists(), ".ocignore file should be created");
    
    // Verify it contains default patterns
    let contents = fs::read_to_string(&ocignore_path).unwrap();
    assert!(contents.contains("node_modules/"), "Should contain node_modules pattern");
    assert!(contents.contains("*.pyc"), "Should contain .pyc pattern");
    assert!(contents.contains(".DS_Store"), "Should contain .DS_Store pattern");
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
