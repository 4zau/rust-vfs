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

/// Changes to stored metadata. A field set to `None` keeps its current value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataUpdate {
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub nlink: Option<u32>,
    pub mtime: Option<u32>,
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
    Unsupported,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "entry not found",
            Self::AlreadyExists => "entry already exists",
            Self::InvalidName => "invalid entry name",
            Self::InvalidPath => "invalid absolute path",
            Self::InvalidMetadata => "invalid metadata attributes",
            Self::NotDirectory => "node is not a directory",
            Self::IsDirectory => "node is a directory",
            Self::DirectoryNotEmpty => "directory is not empty",
            Self::SizeOverflow => "file size exceeds supported range",
            Self::IdentifierExhausted => "node identifiers exhausted",
            Self::Unsupported => "operation is not supported",
        })
    }
}

impl core::error::Error for FsError {}

pub trait FileSystem {
    /// root has to be a directory
    fn root(&self) -> NodeId;
    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId>;
    fn metadata(&self, node: NodeId) -> Result<Metadata>;
    /// applies the supplied fields together, or leaves metadata unchanged on error.
    fn set_metadata(&mut self, _node: NodeId, _update: MetadataUpdate) -> Result<()> {
        Err(FsError::Unsupported)
    }
    /// entries in name order
    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>>;
    fn create(&mut self, _parent: NodeId, _name: &str, _kind: NodeKind) -> Result<NodeId> {
        Err(FsError::Unsupported)
    }
    /// returns bytes copied; reading at or beyond EOF returns zero
    /// tries to fill the entire buffer, but may return fewer bytes if EOF is reached
    fn read(&self, file: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize>;
    /// extends with zeroes as needed, empty writes do not extend files
    fn write(&mut self, _file: NodeId, _offset: usize, _data: &[u8]) -> Result<usize> {
        Err(FsError::Unsupported)
    }
    fn truncate(&mut self, _file: NodeId, _length: usize) -> Result<()> {
        Err(FsError::Unsupported)
    }
    /// removes files or empty directories; deleted IDs become invalid
    fn remove(&mut self, _parent: NodeId, _name: &str) -> Result<()> {
        Err(FsError::Unsupported)
    }
}
