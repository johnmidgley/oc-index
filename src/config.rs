use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

const CONFIG_FILE: &str = "config";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Configuration stored in the .oci directory
#[derive(Debug)]
pub struct Config {
    pub version: String,
}

impl Config {
    /// Create a new config with the current tool version
    pub fn new() -> Self {
        Config {
            version: TOOL_VERSION.to_string(),
        }
    }
    
    /// Save the config to the .oci directory
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let config_path = repo_root.join(crate::index::OCI_DIR).join(CONFIG_FILE);
        let contents = format!("version={}\n", self.version);
        fs::write(&config_path, contents)
            .context("Failed to write config file")?;
        Ok(())
    }
    
    /// Load the config from the .oci directory
    pub fn load(repo_root: &Path) -> Result<Self> {
        let config_path = repo_root.join(crate::index::OCI_DIR).join(CONFIG_FILE);
        
        if !config_path.exists() {
            // For backward compatibility, if config doesn't exist, create one with current version
            let config = Config::new();
            config.save(repo_root)?;
            return Ok(config);
        }
        
        let contents = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        let mut version = TOOL_VERSION.to_string();
        
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                
                match key {
                    "version" => version = value.to_string(),
                    _ => {} // Ignore unknown keys for forward compatibility
                }
            }
        }
        
        Ok(Config { version })
    }
    
    /// Check if the stored version matches the current tool version
    /// Returns true if versions match, false otherwise
    pub fn check_version(&self) -> bool {
        self.version == TOOL_VERSION
    }
    
    /// Display a version mismatch warning
    pub fn warn_version_mismatch(&self) {
        eprintln!("Warning: Index version mismatch!");
        eprintln!("  Index was created with: v{}", self.version);
        eprintln!("  Current tool version:   v{}", TOOL_VERSION);
        eprintln!("  This may cause compatibility issues. Consider running 'oci update' to refresh the index.");
        eprintln!();
    }
}
