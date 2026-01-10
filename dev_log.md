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

3. **Change Detection**: Files are considered unchanged if both size and modified time match. This avoids unnecessary hashing for status checks. Full hash comparison happens during commit.

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