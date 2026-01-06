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


def print_file_infos(directory):
    """
    Prints FileInfo objects for all files in a directory.
    
    Args:
        directory: Path to the directory to scan and print files from.
    """
    for file_info in get_file_infos_with_hash(directory):
        print(file_info)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        directory = "."
    else:
        directory = sys.argv[1]
    
    print_file_infos(directory)

