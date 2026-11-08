use cpio_read::{Error, ErrorKind, read_archive};

const ARCHIVE: &[u8] = include_bytes!("fixtures/sample.cpio");

#[test]
fn reads_archive_created_by_gnu_cpio() {
    let entries = read_archive(ARCHIVE).unwrap();
    assert_eq!(entries.len(), 5);
    let directory = &entries[0];
    assert_eq!(directory.name, "docs");
    assert_eq!(directory.mode & 0o170000, 0o040000);
    assert!(directory.data_range.is_empty());

    for (entry, (name, expected)) in entries[1..].iter().zip([
        (
            "docs/readme.txt",
            &include_bytes!("fixtures/input/docs/readme.txt")[..],
        ),
        ("hello.txt", &include_bytes!("fixtures/input/hello.txt")[..]),
        ("empty.txt", &include_bytes!("fixtures/input/empty.txt")[..]),
        (
            "binary.bin",
            &include_bytes!("fixtures/input/binary.bin")[..],
        ),
    ]) {
        assert_eq!(entry.name, name);
        assert_eq!(entry.mode & 0o170000, 0o100000);
        assert_eq!(&ARCHIVE[entry.data_range.clone()], expected);
    }
}

#[test]
fn rejects_invalid_magic_instead_of_returning_partial_data() {
    let mut bytes = ARCHIVE.to_vec();
    bytes[116..122].copy_from_slice(b"broken");
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::BadMagic,
            offset: 116
        })
    );
    assert_eq!(
        read_archive(b"notcpio"),
        Err(Error {
            kind: ErrorKind::BadMagic,
            offset: 0
        })
    );
}

#[test]
fn rejects_truncated_file_contents() {
    let contents = include_bytes!("fixtures/input/hello.txt");
    let start = ARCHIVE
        .windows(contents.len())
        .position(|data| data == contents)
        .unwrap();
    assert_eq!(
        read_archive(&ARCHIVE[..start + contents.len() - 1])
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
}

#[test]
fn rejects_every_truncation_before_trailer() {
    let trailer = ARCHIVE
        .windows(6)
        .rposition(|bytes| bytes == b"070701")
        .unwrap();
    for end in 0..=trailer {
        assert!(
            read_archive(&ARCHIVE[..end]).is_err(),
            "accepted prefix of {end} bytes"
        );
    }
    assert_eq!(
        read_archive(&ARCHIVE[..trailer]),
        Err(Error {
            kind: ErrorKind::MissingTrailer,
            offset: trailer
        })
    );
}

#[test]
fn rejects_invalid_header_and_name() {
    let mut bytes = ARCHIVE.to_vec();
    bytes[6] = b'Z';
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::InvalidEntry,
            offset: 0
        })
    );
    let mut bytes = ARCHIVE.to_vec();
    bytes[114] = b'x'; // Replace the first name's terminating NUL.
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::InvalidEntry,
            offset: 0
        })
    );
    let mut bytes = ARCHIVE.to_vec();
    bytes[110] = 0xff;
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::InvalidEntry,
            offset: 0
        })
    );
}

#[test]
fn rejects_nonzero_trailing_data() {
    let mut bytes = ARCHIVE.to_vec();
    bytes.push(1);
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::TrailingData,
            offset: ARCHIVE.len()
        })
    );
}

#[test]
fn rejects_nonempty_trailer() {
    let trailer = ARCHIVE
        .windows(6)
        .rposition(|bytes| bytes == b"070701")
        .unwrap();
    let mut bytes = ARCHIVE.to_vec();
    bytes[trailer + 54..trailer + 62].copy_from_slice(b"00000001");
    assert_eq!(
        read_archive(&bytes),
        Err(Error {
            kind: ErrorKind::InvalidEntry,
            offset: trailer
        })
    );
}

#[test]
fn preserves_newc_metadata_fields() {
    let mut bytes = ARCHIVE.to_vec();
    // Distinct values catch swapped fields; keep name and content sizes intact.
    for (index, value) in [
        (0, 0x12345678),
        (1, 0o041750),
        (2, 1000),
        (3, 1001),
        (4, 7),
        (5, u32::MAX),
        (7, 8),
        (8, 9),
        (9, 10),
        (10, 11),
    ] {
        let start = 6 + index * 8;
        bytes[start..start + 8].copy_from_slice(format!("{value:08x}").as_bytes());
    }
    let entries = read_archive(&bytes).unwrap();
    let entry = &entries[0];
    assert_eq!(entry.ino, 0x12345678);
    assert_eq!(entry.mode, 0o041750);
    assert_eq!(entry.uid, 1000);
    assert_eq!(entry.gid, 1001);
    assert_eq!(entry.nlink, 7);
    assert_eq!(entry.mtime, u32::MAX);
    assert_eq!(entry.devmajor, 8);
    assert_eq!(entry.devminor, 9);
    assert_eq!(entry.rdevmajor, 10);
    assert_eq!(entry.rdevminor, 11);
    assert_eq!(entry.name, "docs");
    assert!(entry.data_range.is_empty());
}
