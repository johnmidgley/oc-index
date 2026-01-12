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
fn test_rm_removes_index() {
    let temp_dir = TempDir::new().unwrap();
    run_oci(&["init"], temp_dir.path());
    
    assert!(temp_dir.path().join(".oci").exists());
    
    // Remove without -f should fail
    let (_, stderr, exit_code) = run_oci(&["rm"], temp_dir.path());
    assert_ne!(exit_code, 0);
    assert!(stderr.contains("-f flag is required"));
    
    // Remove with -f should succeed
    let (stdout, _, exit_code) = run_oci(&["rm", "-f"], temp_dir.path());
    assert_eq!(exit_code, 0);
    assert!(stdout.contains("Removed index"));
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
