use crate::{FileSystem, FsError, Metadata, NodeId, NodeKind};
use alloc::{boxed::Box, collections::BTreeMap, string::String};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FsId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    InvalidPath,
    PrefixAlreadyRegistered,
    UnknownFileSystem,
    IdentifierExhausted,
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
    /// .. resolved by removing the last component of the path, or ignored if already at /
    fn parse(input: &str) -> Result<Self> {
        if !input.starts_with('/') || input.contains('\0') {
            return Err(VfsError::InvalidPath);
        }

        let mut name = String::new();
        for component in input.split('/') {
            match component {
                "" | "." => continue,
                ".." => {
                    if let Some(separator) = name.rfind('/') {
                        name.truncate(separator);
                    }
                }
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
            directory: input.ends_with('/') || matches!(input.rsplit('/').next(), Some("." | "..")),
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
        self.registrations
            .remove(&id)
            .map(|_| ())
            .ok_or(VfsError::UnknownFileSystem)
    }

    /// finds the FS and node for the given normalized path
    fn resolve(&self, path: &str) -> Result<(&dyn FileSystem, NodeId)> {
        // pick the longest prefix that matches the path, or / if none
        let entry = self
            .registrations
            .values()
            .filter(|entry| {
                entry.prefix == "/"
                    || path == entry.prefix
                    || path
                        .strip_prefix(entry.prefix.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .max_by_key(|entry| entry.prefix.len())
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
        Ok((entry.fs.as_ref(), node))
    }

    /// returns metadata for the given absolute path
    pub fn metadata(&self, path: &str) -> Result<Metadata> {
        let path = Path::parse(path)?;

        let (fs, node) = self.resolve(&path.name)?;
        let metadata = fs.metadata(node)?;

        // check that path ends with / if and only if the node is a directory
        if path.directory && metadata.kind != NodeKind::Directory {
            return Err(FsError::NotDirectory.into());
        }

        Ok(metadata)
    }
}
