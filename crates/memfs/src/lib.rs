#![no_std]
extern crate alloc;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use vfs_core::{DirEntry, FileSystem, FsError, Metadata, NodeId, NodeKind, Result};

enum Node {
    File(Vec<u8>),
    Directory(BTreeMap<String, NodeId>),
}

impl Node {
    fn kind(&self) -> NodeKind {
        match self {
            Self::File(_) => NodeKind::File,
            Self::Directory(_) => NodeKind::Directory,
        }
    }
}

pub struct MemFs {
    nodes: BTreeMap<NodeId, Node>,
    next_id: Option<u64>,
}

impl MemFs {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(NodeId::new(0), Node::Directory(BTreeMap::new()));
        Self {
            nodes,
            next_id: Some(1),
        }
    }

    fn directory(&self, id: NodeId) -> Result<&BTreeMap<String, NodeId>> {
        match self.nodes.get(&id).ok_or(FsError::NotFound)? {
            Node::Directory(entries) => Ok(entries),
            Node::File(_) => Err(FsError::NotDirectory),
        }
    }

    fn file_mut(&mut self, id: NodeId) -> Result<&mut Vec<u8>> {
        match self.nodes.get_mut(&id).ok_or(FsError::NotFound)? {
            Node::File(bytes) => Ok(bytes),
            Node::Directory(_) => Err(FsError::IsDirectory),
        }
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/']) {
        Err(FsError::InvalidName)
    } else {
        Ok(())
    }
}

fn validate_size(size: usize) -> Result<()> {
    if size > isize::MAX as usize {
        Err(FsError::SizeOverflow)
    } else {
        Ok(())
    }
}

impl FileSystem for MemFs {
    fn root(&self) -> NodeId {
        NodeId::new(0)
    }

    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId> {
        validate_name(name)?;
        self.directory(parent)?
            .get(name)
            .copied()
            .ok_or(FsError::NotFound)
    }

    fn metadata(&self, node: NodeId) -> Result<Metadata> {
        let node = self.nodes.get(&node).ok_or(FsError::NotFound)?;
        Ok(Metadata {
            kind: node.kind(),
            size: match node {
                Node::File(bytes) => bytes.len(),
                Node::Directory(_) => 0,
            },
        })
    }

    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>> {
        self.directory(directory)?
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

    fn create(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> Result<NodeId> {
        validate_name(name)?;

        if self.directory(parent)?.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let value = self.next_id.ok_or(FsError::IdentifierExhausted)?;
        let id = NodeId::new(value);
        let node = match kind {
            NodeKind::File => Node::File(Vec::new()),
            NodeKind::Directory => Node::Directory(BTreeMap::new()),
        };

        self.nodes.insert(id, node);
        
        if let Some(Node::Directory(entries)) = self.nodes.get_mut(&parent) {
            entries.insert(String::from(name), id);
        }

        self.next_id = value.checked_add(1);
        Ok(id)
    }

    fn read(&self, file: NodeId, offset: usize, buffer: &mut [u8]) -> Result<usize> {
        let bytes = match self.nodes.get(&file).ok_or(FsError::NotFound)? {
            Node::File(bytes) => bytes,
            Node::Directory(_) => return Err(FsError::IsDirectory),
        };

        let count = buffer.len().min(bytes.len().saturating_sub(offset));
        if count != 0 {
            buffer[..count].copy_from_slice(&bytes[offset..offset + count]);
        }

        Ok(count)
    }

    fn write(&mut self, file: NodeId, offset: usize, data: &[u8]) -> Result<usize> {
        let bytes = self.file_mut(file)?;

        if data.is_empty() {
            return Ok(0);
        }

        let end = offset
            .checked_add(data.len())
            .ok_or(FsError::SizeOverflow)?;
        validate_size(end)?;

        if end > bytes.len() {
            bytes.resize(end, 0);
        }
        bytes[offset..end].copy_from_slice(data);
        Ok(data.len())
    }

    fn truncate(&mut self, file: NodeId, length: usize) -> Result<()> {
        let bytes = self.file_mut(file)?;
        validate_size(length)?;
        bytes.resize(length, 0);
        Ok(())
    }

    fn remove(&mut self, parent: NodeId, name: &str) -> Result<()> {
        let id = self.lookup(parent, name)?;
        if matches!(self.nodes.get(&id).ok_or(FsError::NotFound)?, 
            Node::Directory(entries) if !entries.is_empty())
        {
            return Err(FsError::DirectoryNotEmpty);
        }
        if let Some(Node::Directory(entries)) = self.nodes.get_mut(&parent) {
            entries.remove(name);
        }
        self.nodes.remove(&id);
        Ok(())
    }
}