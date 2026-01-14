use std::fs;
use std::path::Path;
use anyhow::{Context, Result};
use glob::Pattern;

const OCIGNORE_FILE: &str = "ocignore";

/// Get default ignore patterns as a formatted string for writing to ocignore
/// These are common intermediate/derived files that are typically not tracked
/// Users can modify or remove these patterns as needed
/// 
/// The patterns are loaded from a resource file embedded at compile time
pub fn default_ignore_content() -> String {
    include_str!("default_ocignore").to_string()
}

/// Load ignore patterns from ocignore file
pub fn load_patterns(repo_root: &Path) -> Result<Vec<String>> {
    let ignore_path = repo_root.join(crate::index::OCI_DIR).join(OCIGNORE_FILE);
    
    if !ignore_path.exists() {
        return Ok(Vec::new());
    }
    
    let contents = fs::read_to_string(&ignore_path)
        .context("Failed to read ocignore file")?;
    
    Ok(contents.lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(String::from)
        .collect())
}

/// Initialize ocignore file with default patterns
pub fn init_ignore_file(repo_root: &Path) -> Result<()> {
    let oci_dir = repo_root.join(crate::index::OCI_DIR);
    let ignore_path = oci_dir.join(OCIGNORE_FILE);
    
    // Only write defaults if file doesn't exist
    if !ignore_path.exists() {
        fs::write(&ignore_path, default_ignore_content())
            .context("Failed to create ocignore file")?;
    }
    
    Ok(())
}

/// Add a pattern to the ocignore file
pub fn add_pattern(repo_root: &Path, pattern: &str) -> Result<()> {
    let oci_dir = repo_root.join(crate::index::OCI_DIR);
    fs::create_dir_all(&oci_dir)
        .context("Failed to create .oci directory")?;
    
    let ignore_path = oci_dir.join(OCIGNORE_FILE);
    
    let mut patterns = if ignore_path.exists() {
        fs::read_to_string(&ignore_path)
            .context("Failed to read ocignore file")?
    } else {
        String::new()
    };
    
    if !patterns.is_empty() && !patterns.ends_with('\n') {
        patterns.push('\n');
    }
    
    patterns.push_str(pattern);
    patterns.push('\n');
    
    fs::write(&ignore_path, patterns)
        .context("Failed to write ocignore file")?;
    
    Ok(())
}

/// Check if a path should be ignored based on patterns from ocignore
pub fn should_ignore(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    
    // Always ignore the .oci directory itself
    if path_str.contains("/.oci/") || path_str.ends_with("/.oci") || 
       path_str.starts_with(".oci/") || path_str == ".oci" {
        return true;
    }
    
    for pattern in patterns {
        // Try to match the pattern
        if let Ok(glob_pattern) = Pattern::new(pattern) {
            if glob_pattern.matches(&path_str) {
                return true;
            }
            
            // Also try matching just the file name
            if let Some(file_name) = path.file_name() {
                if glob_pattern.matches(&file_name.to_string_lossy()) {
                    return true;
                }
            }
            
            // For directory patterns (ending with /), check if any parent matches
            if pattern.ends_with('/') {
                // Check if the path or any of its parent directories match the pattern
                let dir_pattern = pattern.trim_end_matches('/');
                
                // Try matching with glob for patterns like *.photoslibrary/resources/derivatives
                if let Ok(glob) = Pattern::new(&format!("{}/**", dir_pattern)) {
                    if glob.matches(&path_str) {
                        return true;
                    }
                }
                
                // Also check literal directory prefix match for simple patterns
                if path_str.starts_with(&format!("{}/", dir_pattern)) {
                    return true;
                }
                
                // Check each parent component
                let mut current = path;
                while let Some(parent) = current.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Ok(glob) = Pattern::new(dir_pattern) {
                        if glob.matches(&parent_str) {
                            return true;
                        }
                    }
                    current = parent;
                }
            }
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_oci_dir() {
        let path = Path::new(".oci/index.json");
        assert!(should_ignore(path, &[]));
    }

    #[test]
    fn test_should_ignore_pattern() {
        let patterns = vec!["*.log".to_string(), "node_modules/".to_string()];
        
        // User patterns should work
        assert!(should_ignore(Path::new("test.log"), &patterns));
        assert!(should_ignore(Path::new("node_modules/package/index.js"), &patterns));
        
        // Test file that's not matched by any pattern
        assert!(!should_ignore(Path::new("test.txt"), &patterns));
    }
    
    #[test]
    fn test_ignore_with_wildcards() {
        let patterns = vec!["*.pyc".to_string(), "*.o".to_string()];
        
        assert!(should_ignore(Path::new("module.pyc"), &patterns));
        assert!(should_ignore(Path::new("lib.o"), &patterns));
        assert!(!should_ignore(Path::new("app.py"), &patterns));
    }
    
    #[test]
    fn test_ignore_directory_patterns() {
        let patterns = vec![".venv/".to_string(), "__pycache__/".to_string()];
        
        assert!(should_ignore(Path::new(".venv/lib/python3.9/site.py"), &patterns));
        assert!(should_ignore(Path::new("__pycache__/module.pyc"), &patterns));
        assert!(!should_ignore(Path::new("venv/requirements.txt"), &patterns));
    }
    
    #[test]
    fn test_no_patterns_ignores_nothing() {
        // With no patterns, only .oci directory should be ignored
        assert!(!should_ignore(Path::new("node_modules/package.json"), &[]));
        assert!(!should_ignore(Path::new("build/output.js"), &[]));
        assert!(!should_ignore(Path::new("file.pyc"), &[]));
        assert!(!should_ignore(Path::new(".DS_Store"), &[]));
    }
}
