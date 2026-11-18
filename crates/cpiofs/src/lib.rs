//! Read-only filesystem backed by CPIO `newc` archive.
#![no_std]

extern crate alloc;
mod cpiofs;

pub use cpiofs::{ArchiveError, CpioFs};
