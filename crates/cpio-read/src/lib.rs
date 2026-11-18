#![no_std]
extern crate alloc;

mod parser;
pub use parser::{read_archive, Error, ErrorKind, Entry};
