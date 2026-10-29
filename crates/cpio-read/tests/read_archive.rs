use cpio_read::{Error, read_archive};

const ARCHIVE: &[u8] = include_bytes!("fixtures/sample.cpio");

#[test]
fn reads_archive_created_by_gnu_cpio() {
    let entries = read_archive(ARCHIVE).unwrap();
    assert_eq!(entries.len(), 5);
    let directory = &entries[0];
    assert_eq!(directory.name, "docs");
    assert_eq!(directory.mode & 0o170000, 0o040000);
    assert!(directory.data.is_empty());

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
        assert_eq!(entry.data, expected);
    }
}

#[test]
fn borrows_file_contents_from_archive() {
    let entries = read_archive(ARCHIVE).unwrap();
    let data = entries
        .iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap()
        .data;
    let start = ARCHIVE
        .windows(data.len())
        .position(|bytes| bytes == data)
        .unwrap();
    assert_eq!(data.as_ptr(), ARCHIVE[start..].as_ptr());
}

#[test]
fn rejects_invalid_magic_instead_of_returning_partial_data() {
    let mut bytes = ARCHIVE.to_vec();
    bytes[116..122].copy_from_slice(b"broken");
    assert_eq!(read_archive(&bytes), Err(Error::BadMagic));
    assert_eq!(read_archive(b"notcpio"), Err(Error::BadMagic));
}

#[test]
fn rejects_truncated_file_contents() {
    let contents = include_bytes!("fixtures/input/hello.txt");
    let start = ARCHIVE
        .windows(contents.len())
        .position(|data| data == contents)
        .unwrap();
    assert_eq!(
        read_archive(&ARCHIVE[..start + contents.len() - 1]),
        Err(Error::Truncated)
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
        Err(Error::MissingTrailer)
    );
}

#[test]
fn rejects_invalid_header_and_name() {
    let mut bytes = ARCHIVE.to_vec();
    bytes[6] = b'Z';
    assert_eq!(read_archive(&bytes), Err(Error::InvalidEntry));
    let mut bytes = ARCHIVE.to_vec();
    bytes[114] = b'x'; // Replace the first name's terminating NUL.
    assert_eq!(read_archive(&bytes), Err(Error::InvalidEntry));
    let mut bytes = ARCHIVE.to_vec();
    bytes[110] = 0xff;
    assert_eq!(read_archive(&bytes), Err(Error::InvalidEntry));
}

#[test]
fn rejects_nonzero_trailing_data() {
    let mut bytes = ARCHIVE.to_vec();
    bytes.push(1);
    assert_eq!(read_archive(&bytes), Err(Error::TrailingData));
}

#[test]
fn rejects_nonempty_trailer() {
    let trailer = ARCHIVE
        .windows(6)
        .rposition(|bytes| bytes == b"070701")
        .unwrap();
    let mut bytes = ARCHIVE.to_vec();
    bytes[trailer + 54..trailer + 62].copy_from_slice(b"00000001");
    assert_eq!(read_archive(&bytes), Err(Error::InvalidEntry));
}
