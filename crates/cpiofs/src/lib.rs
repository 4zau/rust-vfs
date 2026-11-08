//! Read-only filesystem backed by CPIO `newc` archive.
#![no_std]

extern crate alloc;

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::{fmt, ops::Range};
use cpio_read::Entry;
use vfs_core::{DirEntry, FileSystem, FsError, Metadata, NodeId, NodeKind, Result};

const ROOT: NodeId = NodeId::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    Parse(cpio_read::Error),
    InvalidPath(String),
    InvalidEntry(String),
    UnsupportedEntryType(String),
    UnsupportedHardLinks(String),
    DuplicatePath(String),
    PathConflict(String),
    IdentifierExhausted,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::InvalidPath(name) => write!(f, "invalid archive path {name:?}"),
            Self::InvalidEntry(name) => write!(f, "invalid archive entry {name:?}"),
            Self::UnsupportedEntryType(name) => write!(f, "unsupported entry type for {name:?}"),
            Self::UnsupportedHardLinks(name) => {
                write!(f, "hard links are unsupported for {name:?}")
            }
            Self::DuplicatePath(name) => write!(f, "duplicate archive path {name:?}"),
            Self::PathConflict(name) => write!(f, "file and directory conflict at {name:?}"),
            Self::IdentifierExhausted => f.write_str("node identifiers exhausted"),
        }
    }
}

impl core::error::Error for ArchiveError {}

pub struct CpioFs {
    bytes: Vec<u8>,
    nodes: BTreeMap<NodeId, Node>,
}

struct Node {
    metadata: Option<Metadata>,
    data: NodeData,
}

enum NodeData {
    Directory(BTreeMap<String, NodeId>),
    File(Range<usize>),
}

impl Node {
    fn implicit_directory() -> Self {
        Self {
            metadata: None,
            data: NodeData::Directory(BTreeMap::new()),
        }
    }

    fn metadata(&self) -> Metadata {
        self.metadata.unwrap_or(Metadata::new(NodeKind::Directory))
    }

    fn children(&self) -> Result<&BTreeMap<String, NodeId>> {
        match &self.data {
            NodeData::Directory(children) => Ok(children),
            NodeData::File(_) => Err(FsError::NotDirectory),
        }
    }
}

impl CpioFs {
    pub fn from_bytes(bytes: Vec<u8>) -> core::result::Result<Self, ArchiveError> {
        let entries = cpio_read::read_archive(&bytes).map_err(ArchiveError::Parse)?;
        let mut nodes = BTreeMap::from([(ROOT, Node::implicit_directory())]);
        for entry in entries {
            Self::index_entry(&mut nodes, entry)?;
        }
        Ok(Self { bytes, nodes })
    }

    fn index_entry(
        nodes: &mut BTreeMap<NodeId, Node>,
        entry: Entry<'_>,
    ) -> core::result::Result<(), ArchiveError> {
        let kind = match entry.mode & 0o170000 {
            0o100000 if entry.nlink > 1 => {
                return Err(ArchiveError::UnsupportedHardLinks(entry.name.into()));
            }
            0o100000 => NodeKind::File,
            0o040000 if !entry.data_range.is_empty() => {
                return Err(ArchiveError::InvalidEntry(entry.name.into()));
            }
            0o040000 => NodeKind::Directory,
            _ => return Err(ArchiveError::UnsupportedEntryType(entry.name.into())),
        };

        let name = entry.name;
        if name.is_empty() || name.starts_with('/') || name.split('/').any(|part| part == "..") {
            return Err(ArchiveError::InvalidPath(name.into()));
        }
        if kind == NodeKind::File && (name.ends_with('/') || name.rsplit('/').next() == Some(".")) {
            return Err(ArchiveError::InvalidPath(name.into()));
        }

        let mut id = ROOT;
        for component in name
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
        {
            let next_id = nodes.len();
            
            let parent = nodes.get_mut(&id).expect("indexed parent");
            let NodeData::Directory(children) = &mut parent.data else {
                return Err(ArchiveError::PathConflict(name.into()));
            };
            
            if let Some(&child) = children.get(component) {
                id = child;
            } else {
                let child = NodeId::new(
                    u64::try_from(next_id).map_err(|_| ArchiveError::IdentifierExhausted)?,
                );
                children.insert(component.into(), child);
                nodes.insert(child, Node::implicit_directory());
                id = child;
            }
        }

        let node = nodes.get_mut(&id).expect("indexed entry");
        if node.metadata.is_some() {
            return Err(if node.metadata().kind == kind {
                ArchiveError::DuplicatePath(name.into())
            } else {
                ArchiveError::PathConflict(name.into())
            });
        }
        
        let size = entry.data_range.len();
        if kind == NodeKind::File {
            if !node.children().expect("implicit directory").is_empty() {
                return Err(ArchiveError::PathConflict(name.into()));
            }
            node.data = NodeData::File(entry.data_range);
        }
        node.metadata = Some(Metadata {
            kind,
            size,
            mode: entry.mode,
            uid: entry.uid,
            gid: entry.gid,
            nlink: entry.nlink,
            mtime: entry.mtime,
        });
        Ok(())
    }

    fn node(&self, id: NodeId) -> Result<&Node> {
        self.nodes.get(&id).ok_or(FsError::NotFound)
    }
}

impl FileSystem for CpioFs {
    fn root(&self) -> NodeId {
        ROOT
    }

    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err(FsError::InvalidName);
        }
        self.node(parent)?
            .children()?
            .get(name)
            .copied()
            .ok_or(FsError::NotFound)
    }

    fn metadata(&self, node: NodeId) -> Result<Metadata> {
        Ok(self.node(node)?.metadata())
    }

    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>> {
        self.node(directory)?
            .children()?
            .iter()
            .map(|(name, &node)| {
                Ok(DirEntry {
                    name: name.clone(),
                    node,
                    kind: self.metadata(node)?.kind,
                })
            })
            .collect()
    }

    fn read(&self, file: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize> {
        let NodeData::File(range) = &self.node(file)?.data else {
            return Err(FsError::IsDirectory);
        };
        let data = &self.bytes[range.clone()];
        let count = buffer.len().min(data.len().saturating_sub(offset));
        if count != 0 {
            buffer[..count].copy_from_slice(&data[offset..offset + count]);
        }
        Ok(count)
    }
}
