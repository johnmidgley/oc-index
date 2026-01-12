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

# Ignore patterns
oci ignore "*.log"

# Remove the index
oci rm -f
```

The following sections describe the sub-commands available in detail.

## init

To initialize `oci`, switch to the directory you want to index (the repository root) and call

```
oci init
```

This will first check that there is not an existing index (located in the .oci directory). If the directory already exists, the user is warned with an error and the tool exits. If the directory does not exist, then an empty index is created. 

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

where `pattern` is optional and can be a file, directory, or arbirary path pattern (like git). Patterns that are to be ignored are stored in the .oci directory in `.ocignore`. If `pattern` is a relative path, it is expanded to be a path from the root of the repository before added to the ignore file. If `pattern` is ommited, then the current directory is used. 

## status

To check for differences between the index and the file system, use

```
oci status
```

A file is considered not changed if its size and last modified time match the index. The path of any file that has chnaged is output with a '+' prefix to indicate that it exists in the filessytem but not the index, a '-' prefix to indicate it exists in the index but not the filesystem, and an 'M' prefix to indicate the the filesystem version has been modified from what the index contains. 

Files are output in a human readable format with the following fields

```
num_bytes modified sha256 path
```

For each file, ```path``` should be relative to where the command was called. 

 Without the `-r` option, only status for the current dicrectory will be displayed. With the `-r` option, the current directory and all subdirectories will be displayed.

## update

To update the index with any changes from the filesystem, which means updating any fields in the index that have changed (e.g. sha256) call

```oci update [pattern]```

If `pattern` is a file, that single file is updated in the index. If `pattern` is a directory, all files that have changed in that directory and any sub-directories (recursively) are updated in the index. If `pattern` is omitted, the repository root is assumed. 

`update` is done efficiently, only computing hashes for files that have changed, skipping any files that have not changed (i.e. num_bytes and modified haven't changed).

### Output Format

Each file being processed is displayed with a prefix indicating the operation:

- `+` - File is being **added** to the index (new file)
- `U` - File is being **updated** (hash or metadata changed)
- `-` - File is being **removed** from the index (deleted from filesystem)

Example output:
```
+ file1.txt
+ file2.txt
U existing_file.txt
- deleted_file.txt
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

```oci ls [-r]```

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

## rm

To remove an index, call

```
oci rm -f
```

The `-f` flag is required for safety, so if it's not present the tool returns an error to the user.

This deletes the .oci directory.