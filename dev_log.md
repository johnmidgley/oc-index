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

1. **Index Storage**: The index is stored as a JSON file (`.oci/index.json`) for simplicity and human readability. For large repositories, this could be optimized with a binary format or database.

2. **Hash Algorithm**: SHA256 was chosen for file hashing as it provides good collision resistance and is widely used in content-addressable systems.

3. **Change Detection**: Files are considered unchanged if both size and modified time match. This avoids unnecessary hashing for both status checks and updates. The `update` command only recomputes hashes for files that are new or have changed (different size or modified time), making it efficient for incremental updates. Files that haven't changed are skipped and counted separately in the output.

4. **Path Handling**: All paths in the index are stored relative to the repository root for portability. Display paths are made relative to the current working directory for user convenience.

5. **Ignore Patterns**: Uses the `glob` crate for pattern matching, supporting wildcards similar to `.gitignore`.

### Testing

The project includes:
- 7 unit tests covering core functionality (index operations, hashing, pattern matching)
- 8 integration tests that verify end-to-end command behavior

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

### High Priority

**1. `oci duplicates` (or `dupes`)** - Find duplicate files
```bash
oci duplicates [-r]
```
Would show all files with identical hashes (duplicate content), grouped together. Very useful for finding redundant files and saving space. This leverages the hash-based tracking to identify files with identical content regardless of name or location.

**2. `oci verify` - Verify file integrity**
```bash
oci verify [-r]
```
Recompute hashes for indexed files and report any that don't match their stored hash. Useful for detecting corrupted or tampered files. This is a core use case for hash-based file tracking.

**3. `oci stats` - Show index statistics**
```bash
oci stats
```
Display summary information: total files indexed, total size, number of unique hashes, number of duplicates, storage efficiency, etc. Provides quick insight into what's in the index.

**4. `oci prune` - Clean up deleted files**
```bash
oci prune [-n/--dry-run]
```
Remove index entries for files that no longer exist on disk. Similar to `update` but only removes deleted entries without updating existing ones. The dry-run flag would show what would be removed.

**5. `oci export` - Export index data**
```bash
oci export [--format csv|json] [-o output.csv]
```
Export the index to CSV or other formats for external analysis, reporting, or integration with other tools.

### Medium Priority

**6. `oci compare` - Compare directories or indexes**
```bash
oci compare <path1> <path2>
oci compare --other-index <path-to-other-oci>
```
Compare two directories or indexes by hash to find files that exist in both, only in one, or have different content. Useful for directory synchronization analysis.

**7. `oci tree` - Tree view of indexed files**
```bash
oci tree [-r]
```
Show indexed files in a hierarchical tree structure, similar to the Unix `tree` command but filtered to only show indexed files.

**8. `oci find` - Search by filename pattern**
```bash
oci find <pattern>
```
Search indexed files by name pattern (complement to `grep` which searches by hash). Would support glob patterns like `*.txt` or `test*`.

**9. `oci diff` - Show what changed**
```bash
oci diff <file>
```
For a modified file, show a detailed comparison between the indexed version and current version (could integrate with standard diff tools).

**10. `oci history` - Track index changes over time**
```bash
oci history [<file>]
```
If we add an update history feature, this could show how files have changed over time (would require storing historical index snapshots).

### Implementation Notes

- **duplicates** and **verify** provide immediate practical value aligned with the tool's core purpose
- **stats** would be quick to implement and provides useful overview information
- **prune** complements the existing `update` command for index maintenance
- **export** enables integration with other tools and workflows
- Commands like **compare** and **tree** build on the existing data structures
- **history** would require architectural changes to store multiple index versions