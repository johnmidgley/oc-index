# oci

`oci` is a command line tool, written in Rust, that creates an index of files in a directory, including all subdirectories. The purpose of the index is to be able to track files by hash. It is similar to `git`, but does not track any changes; it only cares about file identity. 


To initialize `oci`, switch to the directory you want to index and call

```
oci init
```

This will first check that there is not existing index (located in the .oci directory). If the directory already exists, the user is warned with an error and the tool exits. If the directory does not exist, then it is created and a `index.txt` file is created in the directory. `oci` then recusively traverses all directories and outputs each directory directory path on a line, followed by file entries that look like 
 
```
num_bytes modified sha256 name
```

Where

| Field | Description |
| ----- | ----------- |
| num_bytes  | The file size in bytes |
| modified | The last time the file was modified in epoch time in milliseconds |
| sha256 | The sha256 hash of the file contents |
| basename | The basename of the file |

To remove an index, call

```
oci rm
```