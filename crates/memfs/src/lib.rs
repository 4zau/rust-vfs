#![no_std]
extern crate alloc;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use vfs_core::{DirEntry, FileSystem, FsError, Metadata, MetadataUpdate, NodeId, NodeKind, Result};
mod memfs;
pub use memfs::MemFs;
