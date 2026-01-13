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
- `index.rs` - Core index data structure and persistence (JSON-based storage)
- `file_utils.rs` - File operations including SHA256 hashing, metadata retrieval
- `ignore.rs` - Pattern matching for ignored files (similar to .gitignore)
- `commands.rs` - Implementation of all subcommands

### Design Decisions

1. **Index Storage**: The index is stored as a SQLite database (`.oci/index.db`) for efficiency and scalability. SQLite provides:
   - Compact binary storage (much smaller than JSON)
   - Fast indexed queries by path (primary key) and hash (indexed column)
   - Incremental updates without loading the entire index into memory
   - Ability to handle millions of files efficiently
   - Transaction support for atomic updates
   This design scales well from small projects to full hard drive indexing.

2. **Hash Algorithm**: SHA256 was chosen for file hashing as it provides good collision resistance and is widely used in content-addressable systems.

3. **Change Detection**: Files are considered unchanged if both size and modified time match. This avoids unnecessary hashing for both status checks and updates. The `update` command only recomputes hashes for files that are new or have changed (different size or modified time), making it efficient for incremental updates. Files that haven't changed are skipped and counted separately in the output.

4. **Path Handling**: All paths in the index are stored relative to the repository root for portability. Display paths are made relative to the current working directory for user convenience.

5. **Ignore Patterns**: Uses the `glob` crate for pattern matching, supporting wildcards similar to `.gitignore`. During initialization (`oci init`), a `.ocignore` file is created with conservative default patterns for common intermediate/derived files. These defaults are written to the file (not hardcoded in the application), making them transparent and editable by users. The patterns favor specificity over breadth to avoid false positives:
   - Package manager dependencies and caches (e.g., `node_modules/`, `.npm/`)
   - Tool-specific caches (e.g., `.pytest_cache/`, `.mypy_cache/`)
   - Intermediate compiled files (e.g., `*.o`, `*.class`, `*.pyc`)
   - Framework-specific build directories (e.g., `.next/`, `.nuxt/`)
   - Editor temporary files (e.g., `*.swp`, `*~`)
   
   Generic directory names like `build/`, `dist/`, `bin/`, and `out/` are intentionally NOT included in defaults as they could be legitimate organizational directories. Similarly, final artifacts (executables, libraries) and IDE project files are not included. Users can modify `.ocignore` directly or use `oci ignore [pattern]` to add custom patterns.

### Testing

The project includes:
- 10 unit tests covering core functionality (index operations, hashing, pattern matching)
- 10 integration tests that verify end-to-end command behavior

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

## Future Command Ideas

These are potential commands that could enhance the tool's functionality based on its core purpose of tracking files by hash identity.

### Implemented

**1. `oci duplicates` (or `dupes`)** - Find duplicate files
```bash
oci duplicates
```
Shows all files with identical hashes (duplicate content), grouped together. Very useful for finding redundant files and saving space. This leverages the hash-based tracking to identify files with identical content regardless of name or location. Always searches the entire repository recursively.

**2. `oci stats` - Show index statistics**
```bash
oci stats
```
Displays summary information: total files indexed, total size, number of unique hashes, number of duplicates, storage efficiency, etc. Provides quick insight into what's in the index.