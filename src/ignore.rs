use std::fs;
use std::path::Path;
use anyhow::{Context, Result};
use glob::Pattern;

const OCIGNORE_FILE: &str = ".ocignore";

/// Load ignore patterns from .ocignore file
pub fn load_patterns(repo_root: &Path) -> Result<Vec<String>> {
    let ignore_path = repo_root.join(crate::index::OCI_DIR).join(OCIGNORE_FILE);
    
    if !ignore_path.exists() {
        return Ok(Vec::new());
    }
    
    let contents = fs::read_to_string(&ignore_path)
        .context("Failed to read .ocignore file")?;
    
    Ok(contents.lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(String::from)
        .collect())
}

/// Add a pattern to the .ocignore file
pub fn add_pattern(repo_root: &Path, pattern: &str) -> Result<()> {
    let oci_dir = repo_root.join(crate::index::OCI_DIR);
    fs::create_dir_all(&oci_dir)
        .context("Failed to create .oci directory")?;
    
    let ignore_path = oci_dir.join(OCIGNORE_FILE);
    
    let mut patterns = if ignore_path.exists() {
        fs::read_to_string(&ignore_path)
            .context("Failed to read .ocignore file")?
    } else {
        String::new()
    };
    
    if !patterns.is_empty() && !patterns.ends_with('\n') {
        patterns.push('\n');
    }
    
    patterns.push_str(pattern);
    patterns.push('\n');
    
    fs::write(&ignore_path, patterns)
        .context("Failed to write .ocignore file")?;
    
    Ok(())
}

/// Check if a path should be ignored based on patterns
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
            
            // For directory patterns, check if path starts with the pattern
            if pattern.ends_with('/') {
                let dir_pattern = pattern.trim_end_matches('/');
                if path_str.starts_with(&format!("{}/", dir_pattern)) {
                    return true;
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
        let patterns = vec!["*.log".to_string(), "target/".to_string()];
        
        assert!(should_ignore(Path::new("test.log"), &patterns));
        assert!(should_ignore(Path::new("target/debug/main"), &patterns));
        assert!(!should_ignore(Path::new("test.txt"), &patterns));
    }
}
