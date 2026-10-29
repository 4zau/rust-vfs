//! Read uncompressed CPIO `newc` archives
//!
//! Names and contents borrow the input archive. A zero-sized `TRAILER!!!`
//! entry is required.
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, str};

/// An archive member. `mode` includes Unix file type and permission bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub mode: u32,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadMagic,
    Truncated,
    InvalidEntry,
    SizeOverflow,
    MissingTrailer,
    TrailingData,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadMagic => "expected CPIO newc magic 070701",
            Self::Truncated => "truncated CPIO entry",
            Self::InvalidEntry => "invalid CPIO entry",
            Self::SizeOverflow => "CPIO size exceeds supported range",
            Self::MissingTrailer => "missing CPIO trailer",
            Self::TrailingData => "nonzero data after CPIO trailer",
        })
    }
}

impl core::error::Error for Error {}

pub type CpioResult<T> = Result<T, Error>;

fn align4(value: usize) -> CpioResult<usize> {
    value
        .checked_add(3)
        .map(|n| n & !3)
        .ok_or(Error::SizeOverflow)
}

/// Read all `newc` members, excluding the trailer, without copying file data.
///
/// The caller must keep the archive buffer alive while using returned entries.
pub fn read_archive(mut bytes: &[u8]) -> CpioResult<Vec<Entry<'_>>> {
    let mut entries = Vec::new();

    loop {
        if bytes.is_empty() {
            return Err(Error::MissingTrailer);
        }
        if bytes.get(..6).ok_or(Error::Truncated)? != b"070701" {
            return Err(Error::BadMagic);
        }

        let header = bytes.get(..110).ok_or(Error::Truncated)?;
        let mut fields = [0u32; 13];
        for (field, raw) in fields.iter_mut().zip(header[6..].as_chunks::<8>().0) {
            if !raw.iter().all(u8::is_ascii_hexdigit) {
                return Err(Error::InvalidEntry);
            }

            *field = u32::from_str_radix(str::from_utf8(raw).map_err(|_| Error::InvalidEntry)?, 16)
                .map_err(|_| Error::InvalidEntry)?;
        }

        let size = usize::try_from(fields[6]).map_err(|_| Error::SizeOverflow)?;
        let name_size = usize::try_from(fields[11]).map_err(|_| Error::SizeOverflow)?;
        if name_size == 0 {
            return Err(Error::InvalidEntry);
        }
        let name_end = 110usize.checked_add(name_size).ok_or(Error::SizeOverflow)?;
        let name = bytes.get(110..name_end).ok_or(Error::Truncated)?;
        if name.last() != Some(&0) || name[..name.len() - 1].contains(&0) {
            return Err(Error::InvalidEntry);
        }
        let data_start = align4(name_end)?;
        let data_end = data_start.checked_add(size).ok_or(Error::SizeOverflow)?;
        let next = align4(data_end)?;
        let member = bytes.get(..next).ok_or(Error::Truncated)?;

        if name == b"TRAILER!!!\0" {
            if size != 0 {
                return Err(Error::InvalidEntry);
            }
            // only empty bytes after trailer
            if bytes[next..].iter().any(|&byte| byte != 0) {
                return Err(Error::TrailingData);
            }
            return Ok(entries);
        }

        entries.push(Entry {
            name: str::from_utf8(&name[..name.len() - 1]).map_err(|_| Error::InvalidEntry)?,
            mode: fields[1],
            data: &member[data_start..data_end],
        });

        bytes = &bytes[next..];
    }
}
