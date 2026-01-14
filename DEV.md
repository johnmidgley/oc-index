## Setup
To install Rust:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
Install native debugging tool:

```
Name: CodeLLDB
Id: vadimcn.vscode-lldb
Description: Debugger for native code, powered by LLDB. Debug C++, Rust, and other compiled languages.
Version: 1.12.1
Publisher: vadimcn
VS Marketplace Link: https://marketplace.cursorapi.com/items/?itemName=vadimcn.vscode-lldb
```

## Implementation Notes

### Architecture

The `oci` tool is implemented as a Rust CLI application with the following module structure:

- `main.rs` - CLI argument parsing using `clap` with derive macros
- `index.rs` - Core index data structure and persistence (SQLite-based storage)
- `file_utils.rs` - File operations including SHA256 hashing, metadata retrieval
- `ignore.rs` - Pattern matching for ignored files (similar to .gitignore)
- `config.rs` - Version tracking and configuration management
- `commands.rs` - Implementation of all subcommands

### Design Decisions

TODO - It looks like update and status could be abstracted better to both use a function that reqturns a sequence (is yield supported in rust) of status entries that indicate the state of each file compared to the index. 

1. **Index Storage**: The index is stored as a SQLite database (`.oci/index.db`) for efficiency and scalability. SQLite provides:
   - Compact binary storage (much smaller than JSON)
   - Fast indexed queries by path (primary key) and hash (indexed column)
   - Incremental updates without loading the entire index into memory
   - Ability to handle millions of files efficiently
   - Transaction support for atomic updates
   This design scales well from small projects to full hard drive indexing.

2. **Hash Algorithm**: SHA256 was chosen for file hashing as it provides good collision resistance and is widely used in content-addressable systems.

3. **Change Detection**: Files are considered unchanged if both size and modified time match. This avoids unnecessary hashing for both status checks and updates. The `update` command only recomputes hashes for files that are new or have changed (different size or modified time), making it efficient for incremental updates. Files that haven't changed are skipped and counted separately in the output.

4. **Path Handling**: All paths in the index are stored relative to the repository root for portability. Display paths are made relative to the current working directory for user convenience. When processing user-provided path arguments (especially "." and ".."), paths are canonicalized using `canonicalize()` to resolve:
   - Relative path components like "." and ".."
   - Symlinks (e.g., `/tmp` → `/private/tmp` on macOS)
   - Ensuring consistent path comparison between filesystem scans and index lookups
   
   Without canonicalization, paths like "Google Drive/Papers/./file.txt" won't match "Google Drive/Papers/file.txt" in HashSet lookups, causing files to incorrectly appear as both added and deleted.

5. **Ignore Patterns**: Uses the `glob` crate for pattern matching, supporting wildcards similar to `.gitignore`. During initialization (`oci init`), an `ignore` file is created with conservative default patterns for common intermediate/derived files. These defaults are written to the file (not hardcoded in the application), making them transparent and editable by users. The patterns favor specificity over breadth to avoid false positives:
   - Package manager dependencies and caches (e.g., `node_modules/`, `.npm/`)
   - Tool-specific caches (e.g., `.pytest_cache/`, `.mypy_cache/`)
   - Intermediate compiled files (e.g., `*.o`, `*.class`, `*.pyc`)
   - Framework-specific build directories (e.g., `.next/`, `.nuxt/`)
   - Editor temporary files (e.g., `*.swp`, `*~`)
   
   Generic directory names like `build/`, `dist/`, `bin/`, and `out/` are intentionally NOT included in defaults as they could be legitimate organizational directories. Similarly, final artifacts (executables, libraries) and IDE project files are not included. Users can modify `ignore` directly or use `oci ignore [pattern]` to add custom patterns.

6. **Version Tracking**: The tool maintains a `config` file in the `.oci` directory that stores the version of the tool that created the index. This version is checked on every command execution, and a warning is displayed if there's a mismatch between the index version and the current tool version. The version is obtained at compile time from `Cargo.toml` using `env!("CARGO_PKG_VERSION")` and embedded in the binary. For backward compatibility, if a config file doesn't exist (e.g., in indexes created before this feature was added), one is automatically created with the current tool version. The version checking helps users identify potential compatibility issues when upgrading the tool.

7. **Duplicate File Counting**: The `duplicates` and `stats` commands use consistent methodology for counting duplicates. When files share the same hash (indicating identical content), all files in the duplicate group are counted as duplicates, not just the "extra" copies. For example, if 3 files have identical content, the duplicate count is 3 (not 2). This makes the output consistent between both commands and clearer for users understanding how many files are involved in duplication.

8. **Duplicates Command Scope**: The `duplicates` command always searches the entire repository recursively. The `-r` flag was intentionally removed because checking for duplicates in only a single directory (non-recursive) has limited practical value - duplicate detection is most useful when comparing files across the entire repository structure.

### Testing

The project includes:
- 10 unit tests covering core functionality (index operations, hashing, pattern matching)
- 35 integration tests that verify end-to-end command behavior

All tests pass and cover the major use cases and edge cases.

### Build and Run

```bash
# Build the project
cargo build

# Run tests
cargo test

# Install locally
cargo install --path .

# Run without installing
cargo run -- <command> [args]
```