#![no_std]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use core::{fmt, str};

// shortcut for Result<T, FsError>
pub type Result<T> = core::result::Result<T, FsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub kind: NodeKind,
    pub size: usize, // zero for directories, file size in bytes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub node: NodeId,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    InvalidName,
    InvalidPath,
    NotDirectory,
    IsDirectory,
    DirectoryNotEmpty,
    SizeOverflow,
    IdentifierExhausted,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "entry not found",
            Self::AlreadyExists => "entry already exists",
            Self::InvalidName => "invalid entry name",
            Self::InvalidPath => "invalid absolute path",
            Self::NotDirectory => "node is not a directory",
            Self::IsDirectory => "node is a directory",
            Self::DirectoryNotEmpty => "directory is not empty",
            Self::SizeOverflow => "file size exceeds supported range",
            Self::IdentifierExhausted => "node identifiers exhausted",
        })
    }
}

impl core::error::Error for FsError {}

pub trait FileSystem {
    fn root(&self) -> NodeId;
    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId>;
    fn metadata(&self, node: NodeId) -> Result<Metadata>;
    // entries in name order
    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>>;
    fn create(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> Result<NodeId>;
    // returns bytes copied; reading at or beyond EOF returns zero
    fn read(&self, file: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize>;
    // extends with zeroes as needed, empty writes do not extend files
    fn write(&mut self, file: NodeId, offset: usize, data: &[u8]) -> Result<usize>;
    fn truncate(&mut self, file: NodeId, length: usize) -> Result<()>;
    // removes files or empty directories; deleted IDs become invalid
    fn remove(&mut self, parent: NodeId, name: &str) -> Result<()>;
}
