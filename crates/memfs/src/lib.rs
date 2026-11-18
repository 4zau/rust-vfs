#![no_std]
extern crate alloc;

mod memfs;
pub use memfs::MemFs;

#[cfg(test)]
mod tests;