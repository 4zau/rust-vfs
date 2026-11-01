#![no_std]
extern crate alloc;

mod fs;
mod vfs;

pub use fs::{DirEntry, FileSystem, FsError, Metadata, NodeId, NodeKind, Result};
pub use vfs::{FsId, Vfs, VfsError};
