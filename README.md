# oci

`oci` is a command line tool, written in Rust, that creates an index of files in a directory, including all subdirectories. The purpose of the index is to be able to track files by hash. It is similar to `git`, but does not track any changes; it only cares about file identity. 

## Quick Start

```bash
# Initialize an index in your project directory
oci init

# Add some files
echo "Hello, world!" > test.txt

# Update the index with the files
oci update
# Output: + test.txt
#         Updated 1 file(s) in the index (1 added, 0 updated, 0 removed)

# List indexed files
oci ls

# Check status (shows added, modified, deleted files)
oci status

# Find files by hash
oci grep <hash>

# Find duplicate files
oci duplicates

# Show index statistics
oci stats

# Ignore patterns
oci ignore "*.log"

# Clear all entries from the index (with confirmation)
oci reset

# Or reset without confirmation
oci reset -f

# Remove the index (with confirmation)
oci deinit

# Or remove the index without confirmation
oci deinit -f
```

## Important Note for Google Drive Users

If you're using `oci` to track files in Google Drive, you should configure Google Drive to use **Mirror mode** instead of **Streaming mode**. 

- **Mirror mode**: All files are stored locally on your computer, allowing oci to access and hash them properly
- **Streaming mode**: Files are stored in the cloud and downloaded on-demand, which prevents oci from being able to reliably read and index files

To change this setting, go to Google Drive preferences and select "Mirror files" instead of "Stream files".

The following sections describe the sub-commands available in detail.

## init

To initialize `oci`, switch to the directory you want to index (the repository root) and call

```
oci init
```

This will first check that there is not an existing index (located in the .oci directory). If the directory already exists, the user is warned with an error and the tool exits. If the directory does not exist, then an empty index is created along with:
- A `config` file containing the tool version
- An `ignore` file with default ignore patterns (see the [ignore](#ignore) section below)

The `config` file stores the version of the tool that created the index. This version is `checked` whenever you run any oci command, and a warning is displayed if there's a version mismatch between the index and the current tool version. 

### Version Tracking

The `.oci/config` file stores the version of the tool that created the index. When you run any oci command, the tool checks if the stored version matches the current tool version. If there's a mismatch, you'll see a warning like:

```
Warning: Index version mismatch!
  Index was created with: v0.0.9
  Current tool version:   v0.1.0
  This may cause compatibility issues. Consider running 'oci update' to refresh the index.
```

This warning indicates that the index was created with a different version of oci. While the tool will continue to work, running `oci update` is recommended to ensure the index is up-to-date with the current tool version.

### Index Structure

The index has the following information for each file it tracks:

| Field | Description |
| ----- | ----------- |
| num_bytes  | The file size in bytes |
| modified | The last time the file was modified in epoch time in milliseconds |
| sha256 | The sha256 hash of the file contents |
| path | The full path of the file (for efficiency this may not be explicitly stored, but derived from the location in the index) |

[TODO: Consider a content type field]

The index is organized so that it can efficiently access files for a given directory and can recurse from any directory being tracked, which is required for other commands.

## ignore

For files that should be ignored by oci (i.e. not included in the index and ignored by all commands) call

```
oci ignore [pattern]
```

where `pattern` is optional and can be a file, directory, or arbirary path pattern (like git). Patterns that are to be ignored are stored in the `.oci/ignore` file. If `pattern` is a relative path, it is expanded to be a path from the root of the repository before added to the ignore file. If `pattern` is ommited, then the current directory is used.

### Default Ignore Patterns

When you run `oci init`, an `ignore` file is created with a conservative set of default ignore patterns for common intermediate and derived files. **You can edit this file directly** to add, remove, or modify patterns as needed for your project.

The default patterns include:

**Package Manager Dependencies:**
- `node_modules/`, `bower_components/`, `jspm_packages/`

**Python Intermediate Files:**
- `__pycache__/`, `*.pyc`, `*.pyo`, `*.pyd`, `*.egg-info/`, `.eggs/`

**Python Virtual Environments (dot-prefixed only):**
- `.venv/`, `.env/`

**Python Tool Caches:**
- `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`, `.tox/`

**Intermediate Compiled Files:**
- `*.o`, `*.obj`, `*.class`

**Rust Build Output:**
- `target/debug/`, `target/release/`

**Package Manager Caches:**
- `.npm/`, `.yarn/`, `.gradle/`, `.pnpm-store/`

**Framework-Specific Build Directories:**
- `.next/`, `.nuxt/`, `.svelte-kit/`, `.angular/`, `.cache/`

**Editor Temporary Files:**
- `*.swp`, `*.swo`, `*.swn`, `*~`

**OS Metadata Files:**
- `.DS_Store`, `Thumbs.db`, `desktop.ini`

**Test Coverage Output:**
- `.coverage`, `.nyc_output/`, `htmlcov/`, `__coverage__/`

**macOS System Directories:**
- `.Spotlight-V100/`, `.Trashes/`, `.fseventsd/`, `.TemporaryItems/`, `.DocumentRevisions-V100/`

**Trash Directories:**
- `.Trash/`, `$RECYCLE.BIN/`

**iTunes/Music App Caches:**
- `iTunes/Album Artwork/Cache/`, `Music/Album Artwork/Cache/`

**Photos App Derived Files:**
- `*.photoslibrary/resources/derivatives/`, `*.photoslibrary/resources/proxies/`
- `*.photoslibrary/private/`, `*.photoslibrary/scopes/`
- Note: Originals in `*.photoslibrary/originals/` are NOT ignored

**Browser Caches:**
- Chrome, Firefox, Safari, Edge cache directories (macOS and Windows paths)

**Development Tools:**
- `Library/Developer/Xcode/DerivedData/`, `Library/Developer/Xcode/Archives/`

**Docker:**
- `Library/Containers/com.docker.docker/`

**Cloud Storage Caches:**
- Dropbox, Google Drive, iCloud cache directories

**Mail App Caches:**
- `Library/Mail/V*/MailData/Envelope Index*`, `Library/Mail/V*/MailData/AvailableFeeds/`

**macOS Protected/System Directories:**
- `Library/Application Support/MobileSync/` (iPhone/iPad backups - access restricted by macOS)

**General Cache Locations:**
- `Library/Caches/` (macOS)
- `AppData/Local/Temp/`, `AppData/Local/Cache/` (Windows)

The `.oci` directory itself is always ignored regardless of patterns in `ignore`.

**Important Notes:**
- The default list is **intentionally conservative** to avoid accidentally ignoring legitimate files
- Generic directory names like `build/`, `dist/`, `bin/`, and `out/` are **NOT** included in the defaults
- Final build artifacts (like `*.exe`, `*.dll`, `*.so`, `*.jar`) are **NOT** included in the defaults
- IDE project files (like `.vscode/`, `.idea/`) are **NOT** included in the defaults
- You can remove any default patterns from `ignore` if they don't fit your use case
- You can add custom patterns using `oci ignore [pattern]` or by editing `ignore` directly 

## status

To check for differences between the index and the file system, use

```
oci status [path] [-r] [-v]
```

Where `path` is an optional file or directory to check. If omitted, the entire repository is checked.

A file is considered not changed if its size and last modified time match the index. The path of any file that has changed is output with a prefix indicating its status:

- `+` - File exists in the filesystem but not in the index (new file)
- `-` - File exists in the index but not in the filesystem (deleted file)
- `U` - File has been modified from what the index contains (updated file)
- `=` - File is unchanged (only shown with `-v` flag)
- `I` - File is ignored by patterns in `ignore` (only shown with `-v` flag)

Files are output in a human readable format with the following fields

```
num_bytes modified sha256 path
```

For each file, ```path``` is displayed relative to where the command was called. 

### Behavior

- `oci status` - Checks the entire repository from the root recursively, showing only changed files
- `oci status <path>` - Checks only the specified file or directory (non-recursive for directories)
- `oci status <path> -r` - Checks the specified directory and all subdirectories recursively
- `oci status -r` - Checks from the current directory and its subdirectories recursively
- `oci status -v` - Verbose mode: shows all files including unchanged and ignored files
- `oci status <path> -r -v` - Checks the specified directory recursively and shows all files

## update

To update the index with any changes from the filesystem, which means updating any fields in the index that have changed (e.g. sha256) call

```
oci update [pattern] [-v]
```

If `pattern` is a file, that single file is updated in the index. If `pattern` is a directory, all files that have changed in that directory and any sub-directories (recursively) are updated in the index. If `pattern` is omitted, the repository root is assumed. 

`update` is done efficiently, only computing hashes for files that have changed, skipping any files that have not changed (i.e. num_bytes and modified haven't changed).

### Options

- `-v` - Verbose mode: shows all files including unchanged and ignored files

### Output Format

Each file being processed is displayed with a prefix indicating the operation:

- `+` - File is being **added** to the index (new file)
- `U` - File is being **updated** (hash or metadata changed)
- `-` - File is being **removed** from the index (deleted from filesystem)
- `=` - File is unchanged (only shown with `-v` flag)
- `I` - File is ignored by patterns in `ignore` (only shown with `-v` flag)

Example output:
```
+ file1.txt
+ file2.txt
U existing_file.txt
- deleted_file.txt
Updated 4 file(s) in the index (2 added, 1 updated, 1 removed)
Skipped 5 unchanged file(s)
```

With verbose mode (`-v`):
```
+ file1.txt
+ file2.txt
U existing_file.txt
- deleted_file.txt
= unchanged1.txt
= unchanged2.txt
= unchanged3.txt
= unchanged4.txt
= unchanged5.txt
I node_modules/package.js
I build/output.log
Updated 4 file(s) in the index (2 added, 1 updated, 1 removed)
Skipped 5 unchanged file(s)
```

The summary line shows:
- Total number of files changed (added + updated + removed)
- Breakdown of additions, updates, and removals
- Number of unchanged files that were skipped

Note: The `update` command will automatically remove files from the index that no longer exist on the filesystem within the target directory.

## ls

To list the index for the current directory, call

```
oci ls [-r]
```

Similar to the `status` command, files are output in a human readable format with the following fields

```
num_bytes modified sha256 path
```

The opional `-r` flag causes the command to recurse to all sub-directories.

## grep

To find any files that match a given hash, call:

```
oci grep <hash>
```

Where `<hash>` is the SHA256 hash of the file content you're looking for. This will list all files in the index with that hash. 

## duplicates

To find duplicate files (files with identical content), call:

```
oci duplicates
```

This command identifies all files in the repository that have identical content based on their SHA256 hash. Files are grouped by hash and displayed together.

### Output Format

The command displays:
- Total number of duplicate files and duplicate groups
- Potential space savings (bytes that could be freed by removing all but one copy of each duplicate)
- Each group of duplicates, showing all files with identical content

Example output:
```
Found 4 duplicate file(s) in 2 group(s)
Potential space savings: 2048 bytes (0.00 MB)

Hash: abc123...
  1024 1609459200000 abc123... file1.txt
  1024 1609459200000 abc123... backup/file1_copy.txt

Hash: def456...
  512 1609459200000 def456... data.txt
  512 1609459200000 def456... old/data.txt

```

Note: Files are only considered duplicates if they have identical content (same SHA256 hash). Files with the same name but different content are not considered duplicates.

## stats

To display statistics about the index, call:

```
oci stats
```

This command provides a summary of the indexed files, including:

- **Total files**: The number of files tracked in the index
- **Total size**: The combined size of all indexed files in bytes and MB
- **Unique hashes**: The number of unique content hashes (unique files by content)
- **Duplicate files**: The number of files that are duplicates of other files (total files - unique hashes)
- **Duplicate groups**: The number of groups of duplicate files (only shown if duplicates exist)
- **Wasted space**: The amount of storage consumed by duplicate files (only shown if duplicates exist)
- **Storage efficiency**: The percentage of storage used by unique content (100% means no duplicates)

Example output:
```
Index Statistics:
  Total files: 100
  Total size: 5242880 bytes (5.00 MB)
  Unique hashes: 85
  Duplicate files: 15
  Duplicate groups: 5
  Wasted space: 524288 bytes (0.50 MB)
  Storage efficiency: 90.00%
```

This command is useful for getting a quick overview of your indexed content and identifying potential space savings from duplicate files.

## prune 

If you'd like to remove files based on another index, call

```
oci prune <source>
```

where `<source>` is a path to another `oci` index. If there are any pending updates in either the local or source index (i.e. `status` shows changes), the prune exits with an error. 

If there are no pending changes, the prune command can remove the following types of files:

1. **Duplicate files** - Any file in the local index that is also in the `<source>` index, determined by matching SHA256 hash
2. **Source-ignored files** - Any file in the local index that matches the ignore patterns defined in the `<source>` index's `ignore` file
3. **Local-ignored files** - When the `--ignored` flag is used, any file that matches the ignore patterns defined in the local `ignore` file

All pruned files are moved to `.oci/pruneyard/<path>` where path is the previous relative path to the file in the local index. After moving files, any empty directories are automatically removed. The output shows which files were pruned and the reason:

```
Pruned (duplicate): file1.txt
Pruned (ignored): debug.log
Pruned 2 file(s) to .oci/pruneyard/ (1 duplicates, 1 ignored)
```

### Options

To only prune duplicate files and skip checking the source's ignore patterns, use:

```
oci prune <source> --no-ignore
```

To prune files matching the local `ignore` patterns (in addition to duplicates and source ignore patterns), use:

```
oci prune <source> --ignored
```

To prune only files matching the local `ignore` patterns without comparing to a source index:

```
oci prune --ignored
```

This is useful for cleaning up ignored files from your local repository without needing a source index for comparison.

To restore all pruned files back to their original locations, call:

```
oci prune --restore
```

This will move all files from `.oci/pruneyard/` back to their original locations and add them back to the index. The pruneyard directory is removed after restoration.

To permanently delete pruned files, call:

```
oci prune --purge
```

This command checks for pending changes in the local index before proceeding. If there are pending changes, it exits with an error. If there are no pending changes, it will ask for confirmation before deleting. To skip the confirmation prompt (useful for scripts), use the `-f` or `--force` flag:

```
oci prune --purge -f
```

### Prune Output

When pruning files, oci displays the total size of pruned files in a human-readable format:

```
Pruned 15 file(s) to .oci/pruneyard/ (10 duplicates, 5 ignored, 2.35 MB)
```

The size is automatically formatted in the most appropriate unit (bytes, KB, MB, or GB).

## reset

To clear all entries from the index while keeping the `.oci` directory structure intact, call

```
oci reset
```

This will ask for confirmation before clearing the index. To skip the confirmation prompt (useful for scripts), use the `-f` flag:

```
oci reset -f
```

This command removes all file entries from the index database but preserves the `.oci` directory, `config`, and `ignore` files. After reset:
- The index will be empty (like a freshly initialized index)
- Your files on the filesystem remain untouched
- The `.oci` directory structure is preserved
- You can run `oci update` to re-index files

This is useful when you want to start fresh with the index without losing your ignore patterns or having to reinitialize.

## deinit

To deinitialize and remove an index, call

```
oci deinit
```

This will ask for confirmation before deleting the `.oci` directory. To skip the confirmation prompt (useful for scripts), use the `-f` flag:

```
oci deinit -f
```

This command deletes the `.oci` directory, which is the opposite of `init`.