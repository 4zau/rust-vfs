#![no_std]
extern crate alloc;

mod fs;
mod vfs;

pub use fs::{DirEntry, FileSystem, FsError, Metadata, MetadataUpdate, NodeId, NodeKind, Result};
pub use vfs::{FileDescriptor, FsId, Vfs, VfsError};
