use crate::{FileSystem, FsError, Metadata, NodeId, NodeKind};
use alloc::{boxed::Box, collections::BTreeMap, string::String};

// a lot of things here use u64 identifiers, none of them are reused
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FsId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileDescriptor(u64);

struct OpenFile {
    fs_id: FsId,
    node: NodeId,
    offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    InvalidPath,
    PrefixAlreadyRegistered,
    UnknownFileSystem,
    IdentifierExhausted,
    InvalidDescriptor,
    Busy,
    InvalidBackendResult,
    Fs(FsError),
}

impl From<FsError> for VfsError {
    fn from(error: FsError) -> Self {
        Self::Fs(error)
    }
}

impl core::fmt::Display for VfsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPath => f.write_str("invalid absolute path"),
            Self::PrefixAlreadyRegistered => f.write_str("prefix already registered"),
            Self::UnknownFileSystem => f.write_str("unknown filesystem"),
            Self::IdentifierExhausted => f.write_str("filesystem identifiers exhausted"),
            Self::InvalidDescriptor => f.write_str("invalid file descriptor"),
            Self::Busy => f.write_str("filesystem has open files"),
            Self::InvalidBackendResult => f.write_str("filesystem returned an invalid byte count"),
            Self::Fs(error) => error.fmt(f),
        }
    }
}
impl core::error::Error for VfsError {}

type Result<T> = core::result::Result<T, VfsError>;

struct Path {
    name: String,
    directory: bool,
}
impl Path {
    /// normalizes path before choosing a FS, but does not check that the path exists in the FS
    /// requires an absolute path, and does not allow null bytes
    /// rejects any .. component; repeated / and . are normalized
    fn parse(input: &str) -> Result<Self> {
        if !input.starts_with('/') || input.contains('\0') {
            return Err(VfsError::InvalidPath);
        }

        let mut name = String::new();
        for component in input.split('/') {
            match component {
                "" | "." => continue,
                ".." => return Err(VfsError::InvalidPath),
                _ => {
                    name.push('/');
                    name.push_str(component);
                }
            }
        }

        if name.is_empty() {
            name.push('/');
        }

        Ok(Self {
            name,
            directory: input.ends_with('/') || matches!(input.rsplit('/').next(), Some(".")),
        })
    }
}

struct Registration {
    prefix: String,
    fs: Box<dyn FileSystem>,
}

#[derive(Default)]
pub struct Vfs {
    registrations: BTreeMap<FsId, Registration>,
    next_id: u64,
    // one large fd table for everyone.
    open_files: BTreeMap<FileDescriptor, OpenFile>,
    next_fd: u64,
}
impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// registers FS under prefix, which must be absolute
    pub fn register(&mut self, prefix: &str, fs: Box<dyn FileSystem>) -> Result<FsId> {
        let prefix = Path::parse(prefix)?.name;

        if self
            .registrations
            .values()
            .any(|entry| entry.prefix == prefix)
        {
            return Err(VfsError::PrefixAlreadyRegistered);
        }

        if fs.metadata(fs.root())?.kind != NodeKind::Directory {
            return Err(FsError::NotDirectory.into());
        }

        let next = self
            .next_id
            .checked_add(1)
            .ok_or(VfsError::IdentifierExhausted)?;

        let id = FsId(self.next_id);
        self.registrations.insert(id, Registration { prefix, fs });
        self.next_id = next;

        Ok(id)
    }

    pub fn unregister(&mut self, id: FsId) -> Result<()> {
        if !self.registrations.contains_key(&id) {
            return Err(VfsError::UnknownFileSystem);
        }

        if self.open_files.values().any(|file| file.fs_id == id) {
            return Err(VfsError::Busy);
        }

        self.registrations
            .remove(&id)
            .map(|_| ())
            .ok_or(VfsError::UnknownFileSystem)
    }

    /// finds the FS and node for the given normalized path
    fn resolve(&self, path: &Path) -> Result<(FsId, &dyn FileSystem, NodeId)> {
        let path = path.name.as_str();
        // pick the longest prefix that matches the path, or / if none match
        let (id, entry) = self
            .registrations
            .iter()
            .filter(|(_, entry)| {
                entry.prefix == "/"
                    || path == entry.prefix
                    || path
                        .strip_prefix(entry.prefix.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .max_by_key(|(_, entry)| entry.prefix.len())
            .ok_or(FsError::NotFound)?;

        let mut node = entry.fs.root();
        let tail = if entry.prefix == "/" {
            path
        } else {
            &path[entry.prefix.len()..]
        };

        for component in tail.split('/').filter(|part| !part.is_empty()) {
            if entry.fs.metadata(node)?.kind != NodeKind::Directory {
                return Err(FsError::NotDirectory.into());
            }
            node = entry.fs.lookup(node, component)?;
        }
        Ok((*id, entry.fs.as_ref(), node))
    }

    /// finds mutable FS and node for modifying operations
    fn resolve_mut(&mut self, path: &Path) -> Result<(FsId, &mut (dyn FileSystem + 'static), NodeId)> {
        let path_str = path.name.as_str();
        let id = self
            .registrations
            .iter()
            .filter(|(_, entry)| {
                entry.prefix == "/"
                    || path_str == entry.prefix
                    || path_str
                        .strip_prefix(entry.prefix.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .max_by_key(|(_, entry)| entry.prefix.len())
            .map(|(&id, _)| id)
            .ok_or(FsError::NotFound)?;

        let entry = self.registrations.get_mut(&id).unwrap();
        let mut node = entry.fs.root();
        let tail = if entry.prefix == "/" {
            path_str
        } else {
            &path_str[entry.prefix.len()..]
        };

        for component in tail.split('/').filter(|part| !part.is_empty()) {
            if entry.fs.metadata(node)?.kind != NodeKind::Directory {
                return Err(FsError::NotDirectory.into());
            }
            node = entry.fs.lookup(node, component)?;
        }
        Ok((id, entry.fs.as_mut(), node))
    }

    /// returns metadata for the given absolute path
    pub fn metadata(&self, path: &str) -> Result<Metadata> {
        let path = Path::parse(path)?;

        let (_, fs, node) = self.resolve(&path)?;
        let metadata = fs.metadata(node)?;

        // trailing "/" requires a directory, however a directory doesnt require a trailing "/"
        if path.directory && metadata.kind != NodeKind::Directory {
            return Err(FsError::NotDirectory.into());
        }

        Ok(metadata)
    }

    /// replaces stored metadata for the given absolute path
    pub fn set_metadata(&mut self, path: &str, metadata: Metadata) -> Result<()> {
        let path = Path::parse(path)?;
        let (_, fs, node) = self.resolve_mut(&path)?;

        if path.directory && metadata.kind != NodeKind::Directory {
            return Err(FsError::NotDirectory.into());
        }

        fs.set_metadata(node, metadata)?;
        Ok(())
    }

    /// opens a regular file with an independent read position, initially equal to zero.
    pub fn open(&mut self, path: &str) -> Result<FileDescriptor> {
        let path = Path::parse(path)?;
        let (fs_id, fs, node) = self.resolve(&path)?;
        let metadata = fs.metadata(node)?;

        if metadata.kind == NodeKind::Directory {
            return Err(FsError::IsDirectory.into());
        }
        if path.directory {
            return Err(FsError::NotDirectory.into());
        }

        let next = self
            .next_fd
            .checked_add(1)
            .ok_or(VfsError::IdentifierExhausted)?;

        let fd = FileDescriptor(self.next_fd);
        self.open_files.insert(
            fd,
            OpenFile {
                fs_id,
                node,
                offset: 0,
            },
        );

        self.next_fd = next;
        Ok(fd)
    }

    /// reads from the current position. on error, the position remains unchanged.
    pub fn read(&mut self, fd: FileDescriptor, buffer: &mut [u8]) -> Result<usize> {
        let file = self
            .open_files
            .get_mut(&fd)
            .ok_or(VfsError::InvalidDescriptor)?;

        let fs = &self
            .registrations
            .get(&file.fs_id)
            .ok_or(VfsError::UnknownFileSystem)?
            .fs;

        let count = fs.read(file.node, file.offset, buffer)?;
        if count > buffer.len() {
            return Err(VfsError::InvalidBackendResult);
        }

        let offset = file
            .offset
            .checked_add(count)
            .ok_or(FsError::SizeOverflow)?;
        file.offset = offset;

        Ok(count)
    }

    /// closes one open file.
    pub fn close(&mut self, fd: FileDescriptor) -> Result<()> {
        self.open_files
            .remove(&fd)
            .map(|_| ())
            .ok_or(VfsError::InvalidDescriptor)
    }
}