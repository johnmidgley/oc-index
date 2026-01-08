import hashlib
import os
import sys
from datetime import datetime


class FileInfo:
    """Represents information about a file."""
    
    def __init__(self, filename, last_modified, file_size):
        self.filename = filename
        self.last_modified = last_modified
        self.file_size = file_size
    
    def __str__(self):
        """Returns a formatted string with size, last_modified, and name."""
        # Format size with fixed width (12 characters)
        size_str = f"{self.file_size:>12,} bytes"
        
        # Format last_modified timestamp with fixed width (20 characters)
        dt = datetime.fromtimestamp(self.last_modified)
        modified_str = dt.strftime("%Y-%m-%d %H:%M:%S")
        
        return f"{size_str}  {modified_str:20s}  {self.filename}"


class FileInfoWithHash(FileInfo):
    """Represents information about a file, including its hash."""
    
    def __init__(self, filename, last_modified, file_size, hash_value):
        super().__init__(filename, last_modified, file_size)
        self.hash = hash_value
    
    def __str__(self):
        """Returns a formatted string with size, last_modified, hash, and name."""
        # Format size with fixed width (12 characters)
        size_str = f"{self.file_size:>12,} bytes"
        
        # Format last_modified timestamp with fixed width (20 characters)
        dt = datetime.fromtimestamp(self.last_modified)
        modified_str = dt.strftime("%Y-%m-%d %H:%M:%S")
        
        # Format hash with fixed width (64 characters for SHA-256)
        hash_str = f"{self.hash:64s}"
        
        return f"{size_str}  {modified_str:20s}  {hash_str}  {self.filename}"


class DirectoryInfo:
    """Represents information about a directory."""
    
    def __init__(self, path, file_infos, directory_infos=None):
        self.path = path
        self.file_infos = file_infos
        self.directory_infos = directory_infos if directory_infos is not None else []
    
    def __str__(self):
        """Returns the path."""
        return self.path
    

def compute_file_hash(filepath):
    """
    Computes the SHA-256 hash of a file.
    
    Args:
        filepath: Path to the file to compute the hash for.
    
    Returns:
        Hexadecimal string representation of the SHA-256 hash.
    """
    sha256_hash = hashlib.sha256()
    with open(filepath, "rb") as f:
        for chunk in iter(lambda: f.read(4096), b""):
            sha256_hash.update(chunk)
    return sha256_hash.hexdigest()


def get_file_infos(directory):
    """
    Returns FileInfo objects for all files in a directory.
    
    Args:
        directory: Path to the directory to scan for files.
    
    Yields:
        FileInfo object for each file in the directory.
    """
    for filename in os.listdir(directory):
        filepath = os.path.join(directory, filename)
        if os.path.isfile(filepath):
            last_modified = os.path.getmtime(filepath)
            file_size = os.path.getsize(filepath)
            yield FileInfo(filename, last_modified, file_size)


def get_file_infos_with_hash(directory):
    """
    Returns FileInfoWithHash objects for all files in a directory.
    Uses get_file_infos and computes the SHA-256 hash for each file.
    
    Args:
        directory: Path to the directory to scan for files.
    
    Yields:
        FileInfoWithHash object for each file in the directory.
    """
    for file_info in get_file_infos(directory):
        filepath = os.path.join(directory, file_info.filename)
        hash_value = compute_file_hash(filepath)
        yield FileInfoWithHash(file_info.filename, file_info.last_modified, file_info.file_size, hash_value)


def build_index(path):
    """
    Recursively builds a DirectoryInfo tree starting from the given path.
    Each DirectoryInfo contains FileInfos populated by get_file_infos.
    
    Args:
        path: Root path to start building the index from.
    
    Returns:
        DirectoryInfo containing the directory structure with nested DirectoryInfos.
    """
    
    print(f"{path}") 

    # Convert path to absolute path for consistent handling
    abs_path = os.path.abspath(path)
    
    # Get FileInfos for files in the current directory
    file_infos = list(get_file_infos(abs_path))
    
    # Recursively build DirectoryInfos for subdirectories
    directory_infos = []
    try:
        for filename in os.listdir(abs_path):
            subdir_path = os.path.join(abs_path, filename)
            if os.path.isdir(subdir_path):
                subdir_info = build_index(subdir_path)
                directory_infos.append(subdir_info)
    except PermissionError:
        # Skip directories we don't have permission to read
        pass
    
    return DirectoryInfo(abs_path, file_infos, directory_infos)

def print_directory_info(directory_info):
    """
    Prints the index in a tree-like format.
    """
    print(directory_info.path)
    for file_info in directory_info.file_infos:
        print(file_info)
    for subdir_info in directory_info.directory_infos:
        print_directory_info(subdir_info) 


if __name__ == "__main__":
    if len(sys.argv) < 2:
        directory = "."
    else:
        directory = sys.argv[1]
    
    directory_info = build_index(directory)
    print_directory_info(directory_info)

