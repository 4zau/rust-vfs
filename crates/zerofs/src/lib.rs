#![no_std]
extern crate alloc;

use alloc::{string::String, vec, vec::Vec};
use vfs_core::{DirEntry, FileSystem, FsError, Metadata, NodeId, NodeKind, Result};

const ROOT: NodeId = NodeId::new(0);
const ZERO: NodeId = NodeId::new(1);

/// /dev/zero like system
/// mount at /dev to get an equivalent of /dev/zero
#[derive(Debug, Default, Clone, Copy)]
pub struct ZeroFs;

impl ZeroFs {
    pub const fn new() -> Self {
        Self
    }

    fn check_file(file: NodeId) -> Result<()> {
        match file {
            ZERO => Ok(()),
            ROOT => Err(FsError::IsDirectory),
            _ => Err(FsError::NotFound),
        }
    }

    fn check_directory(directory: NodeId) -> Result<()> {
        match directory {
            ROOT => Ok(()),
            ZERO => Err(FsError::NotDirectory),
            _ => Err(FsError::NotFound),
        }
    }
}

impl FileSystem for ZeroFs {
    fn root(&self) -> NodeId {
        ROOT
    }

    fn lookup(&self, parent: NodeId, name: &str) -> Result<NodeId> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return Err(FsError::InvalidName);
        }
        Self::check_directory(parent)?;
        match name {
            "zero" => Ok(ZERO),
            _ => Err(FsError::NotFound),
        }
    }

    fn metadata(&self, node: NodeId) -> Result<Metadata> {
        match node {
            ROOT => Ok(Metadata::new(NodeKind::Directory)),
            ZERO => Ok(Metadata {
                mode: 0o100666,
                ..Metadata::new(NodeKind::File)
            }),
            _ => Err(FsError::NotFound),
        }
    }

    fn read_dir(&self, directory: NodeId) -> Result<Vec<DirEntry>> {
        Self::check_directory(directory)?;
        Ok(vec![DirEntry {
            name: String::from("zero"),
            node: ZERO,
            kind: NodeKind::File,
        }])
    }

    /// fills the entire buffer with zeroes
    fn read(&self, file: NodeId, _offset: usize, buffer: &mut [u8]) -> Result<usize> {
        Self::check_file(file)?;
        buffer.fill(0);
        Ok(buffer.len())
    }

    /// accepts all bytes without storing them
    fn write(&mut self, file: NodeId, _offset: usize, data: &[u8]) -> Result<usize> {
        Self::check_file(file)?;
        Ok(data.len())
    }
}
