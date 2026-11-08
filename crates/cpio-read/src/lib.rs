//! Read uncompressed CPIO `newc` archives
//!
//! Names borrow the input archive. Contents are represented by byte ranges.
//! A zero-sized `TRAILER!!!` entry is required.
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, ops::Range, str};

/// An archive member. `mode` includes Unix file type and permission bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    /// Seconds since the Unix epoch.
    pub mtime: u32,
    pub devmajor: u32,
    pub devminor: u32,
    pub rdevmajor: u32,
    pub rdevminor: u32,
    /// Byte range of the file contents relative to the beginning of the archive.
    pub data_range: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadMagic,
    Truncated,
    InvalidEntry,
    SizeOverflow,
    MissingTrailer,
    TrailingData,
}

impl fmt::Display for ErrorKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// Byte offset in the archive. For entry errors, this is the start of the header.
    pub offset: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.offset)
    }
}

impl core::error::Error for Error {}

pub type CpioResult<T> = Result<T, Error>;

fn align4(value: usize) -> Result<usize, ErrorKind> {
    value
        .checked_add(3)
        .map(|n| n & !3)
        .ok_or(ErrorKind::SizeOverflow)
}

/// Read all `newc` members, excluding the trailer, without copying file data.
///
/// Entry names borrow from the archive buffer.
pub fn read_archive(mut bytes: &[u8]) -> CpioResult<Vec<Entry<'_>>> {
    let mut entries = Vec::new();
    let mut offset = 0;

    while !bytes.is_empty() {
        let (entry, next) = read_member(bytes, offset).map_err(|kind| Error { kind, offset })?;
        let Some(entry) = entry else {
            if let Some(index) = bytes[next..].iter().position(|&byte| byte != 0) {
                return Err(Error {
                    kind: ErrorKind::TrailingData,
                    offset: offset + next + index,
                });
            }
            return Ok(entries);
        };
        entries.push(entry);
        bytes = &bytes[next..];
        offset += next;
    }

    Err(Error {
        kind: ErrorKind::MissingTrailer,
        offset,
    })
}

fn read_member(bytes: &[u8], offset: usize) -> Result<(Option<Entry<'_>>, usize), ErrorKind> {
    if bytes.get(..6).ok_or(ErrorKind::Truncated)? != b"070701" {
        return Err(ErrorKind::BadMagic);
    }

    let header = bytes.get(..110).ok_or(ErrorKind::Truncated)?;
    let mut fields = [0u32; 13];
    for (field, raw) in fields.iter_mut().zip(header[6..].as_chunks::<8>().0) {
        if !raw.iter().all(u8::is_ascii_hexdigit) {
            return Err(ErrorKind::InvalidEntry);
        }

        *field = u32::from_str_radix(
            str::from_utf8(raw).map_err(|_| ErrorKind::InvalidEntry)?,
            16,
        )
        .map_err(|_| ErrorKind::InvalidEntry)?;
    }

    let size = usize::try_from(fields[6]).map_err(|_| ErrorKind::SizeOverflow)?;
    let name_size = usize::try_from(fields[11]).map_err(|_| ErrorKind::SizeOverflow)?;
    if name_size == 0 {
        return Err(ErrorKind::InvalidEntry);
    }
    let name_end = 110usize
        .checked_add(name_size)
        .ok_or(ErrorKind::SizeOverflow)?;
    let name = bytes.get(110..name_end).ok_or(ErrorKind::Truncated)?;
    if name.last() != Some(&0) || name[..name.len() - 1].contains(&0) {
        return Err(ErrorKind::InvalidEntry);
    }
    let data_start = align4(name_end)?;
    let data_end = data_start
        .checked_add(size)
        .ok_or(ErrorKind::SizeOverflow)?;
    let next = align4(data_end)?;
    bytes.get(..next).ok_or(ErrorKind::Truncated)?;

    if name == b"TRAILER!!!\0" {
        if size != 0 {
            return Err(ErrorKind::InvalidEntry);
        }
        return Ok((None, next));
    }

    let entry = Entry {
        name: str::from_utf8(&name[..name.len() - 1]).map_err(|_| ErrorKind::InvalidEntry)?,
        ino: fields[0],
        mode: fields[1],
        uid: fields[2],
        gid: fields[3],
        nlink: fields[4],
        mtime: fields[5],
        devmajor: fields[7],
        devminor: fields[8],
        rdevmajor: fields[9],
        rdevminor: fields[10],
        data_range: offset + data_start..offset + data_end,
    };

    Ok((Some(entry), next))
}
