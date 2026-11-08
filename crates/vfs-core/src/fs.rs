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
    /// unix file type and permission bits, as stored in CPIO newc.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// stored link count; hard links are not supported though.
    pub nlink: u32,
    /// seconds since the Unix epoch; no automatic clock updates either.
    pub mtime: u32,
}

impl Metadata {
    /// returns metadata for an empty node owned by root, with mode 0644 or 0755.
    /// the modification timestamp defaults to zero.
    pub const fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            size: 0,
            mode: match kind {
                NodeKind::File => 0o100644,
                NodeKind::Directory => 0o040755,
            },
            uid: 0,
            gid: 0,
            nlink: 1,
            mtime: 0,
        }
    }
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
    InvalidMetadata,
    NotDirectory,
    IsDirectory,
    DirectoryNotEmpty,
    SizeOverflow,
    IdentifierExhausted,
    ReadOnly,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "entry not found",
            Self::AlreadyExists => "entry already exists",
            Self::InvalidName => "invalid entry name",
            Self::InvalidPath => "invalid absolute path",
            Self::InvalidMetadata => "metadata does not match node type or size",
            Self::NotDirectory => "node is not a directory",
            Self::IsDirectory => "node is a directory",
            Self::DirectoryNotEmpty => "directory is not empty",
            Self::SizeOverflow => "file size exceeds supported range",
            Self::IdentifierExhausted => "node identifiers exhausted",
            Self::ReadOnly => "filesystem is read-only",
        })
    }
}

impl core::error::Error for FsError {}

pub trait FileSystem {
    /// root has to be a directory
    fn root(&self) -> NodeId;
    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId>;
    fn metadata(&self, node: NodeId) -> Result<Metadata>;
        fn set_metadata(&mut self, _node: NodeId, _metadata: Metadata) -> Result<()> {
        Err(FsError::ReadOnly)
    }
    /// entries in name order
    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>>;
    fn create(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> Result<NodeId>;
    /// returns bytes copied; reading at or beyond EOF returns zero
    /// tries to fill the entire buffer, but may return fewer bytes if EOF is reached
    fn read(&self, file: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize>;
    /// extends with zeroes as needed, empty writes do not extend files
    fn write(&mut self, file: NodeId, offset: usize, data: &[u8]) -> Result<usize>;
    fn truncate(&mut self, file: NodeId, length: usize) -> Result<()>;
    /// removes files or empty directories; deleted IDs become invalid
    fn remove(&mut self, parent: NodeId, name: &str) -> Result<()>;
}
