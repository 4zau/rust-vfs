//! Read uncompressed CPIO `newc` archives
//!
//! Names borrow the input archive. Contents are represented by byte ranges.
//! A zero-sized `TRAILER!!!` entry is required.
#![no_std]
extern crate alloc;

mod parser;
pub use parser::{CpioResult, Entry, Error, ErrorKind, read_archive};
