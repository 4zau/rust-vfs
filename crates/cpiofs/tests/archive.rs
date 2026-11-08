use cpiofs::{ArchiveError, CpioFs};
use vfs_core::{FileSystem, FsError, MetadataUpdate, Vfs, VfsError};

const SAMPLE: &[u8] = include_bytes!("../../cpio-read/tests/fixtures/sample.cpio");
const README: &[u8] = include_bytes!("../../cpio-read/tests/fixtures/input/docs/readme.txt");
const FILE: u32 = 0o100644;
const DIR: u32 = 0o040755;

#[test]
fn reads_sample_archive_through_vfs() {
    let mut vfs = Vfs::new();
    let fs = CpioFs::from_bytes(SAMPLE.to_vec()).unwrap();
    vfs.register("/initrd", Box::new(fs)).unwrap();
    let docs = vfs.read_dir("/initrd/docs").unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].name, "readme.txt");

    let path = "/initrd/docs/readme.txt";
    assert_eq!(vfs.metadata(path).unwrap().size, README.len());
    let fd = vfs.open(path).unwrap();
    let mut buffer = [0; 3];
    let mut data = Vec::new();
    loop {
        let count = vfs.read(fd, &mut buffer).unwrap();
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
    }
    vfs.close(fd).unwrap();
    assert_eq!(data, README);
    assert_eq!(
        vfs.set_metadata(path, MetadataUpdate::default()),
        Err(VfsError::Fs(FsError::Unsupported))
    );
}

fn archive(entries: &[(&str, u32, u32, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (name, mode, nlink, data) in entries
        .iter()
        .copied()
        .chain([("TRAILER!!!", 0, 1, &[][..])])
    {
        bytes.extend_from_slice(b"070701");
        for field in [
            1,
            mode,
            1000,
            1001,
            nlink,
            123,
            data.len() as u32,
            0,
            0,
            0,
            0,
            (name.len() + 1) as u32,
            0,
        ] {
            bytes.extend_from_slice(format!("{field:08x}").as_bytes());
        }
        bytes.extend_from_slice(name.as_bytes());
        bytes.resize((bytes.len() + 1).next_multiple_of(4), 0);
        bytes.extend_from_slice(data);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
    }
    bytes
}

#[test]
fn indexes_implicit_directories_and_applies_later_metadata() {
    let fs = CpioFs::from_bytes(archive(&[
        ("./docs//nested/file", FILE, 1, b"abc"),
        ("docs/", 0o040700, 2, b""),
        (".", DIR, 2, b""),
    ]))
    .unwrap();
    let root = fs.root();
    assert_eq!(fs.metadata(root).unwrap().nlink, 2);

    let docs = fs.lookup(root, "docs").unwrap();
    let metadata = fs.metadata(docs).unwrap();
    assert_eq!(
        (metadata.mode, metadata.uid, metadata.gid),
        (0o040700, 1000, 1001)
    );
    assert_eq!(metadata.mtime, 123);
    let nested = fs.lookup(docs, "nested").unwrap();
    assert_eq!(fs.metadata(nested).unwrap().mode, DIR);
    let file = fs.lookup(nested, "file").unwrap();

    let mut buffer = [0xff; 3];
    assert_eq!(fs.read(file, 2, &mut buffer), Ok(1));
    assert_eq!(buffer, [b'c', 0xff, 0xff]);
    assert_eq!(fs.read(file, usize::MAX, &mut buffer), Ok(0));
    assert_eq!(fs.read(file, 0, &mut []), Ok(0));
    assert_eq!(fs.read_dir(file), Err(FsError::NotDirectory));
    assert_eq!(fs.read(nested, 0, &mut buffer), Err(FsError::IsDirectory));
}

#[test]
fn rejects_invalid_paths_and_unsupported_entries() {
    for name in ["", "/absolute", "dir/../file", "file/", "file/."] {
        assert_eq!(
            CpioFs::from_bytes(archive(&[(name, FILE, 1, b"")])).err(),
            Some(ArchiveError::InvalidPath(name.into()))
        );
    }
    for (name, mode, nlink, data, error) in [
        (
            "link",
            0o120777,
            1,
            &b"file"[..],
            ArchiveError::UnsupportedEntryType("link".into()),
        ),
        (
            "hardlink",
            FILE,
            2,
            &b""[..],
            ArchiveError::UnsupportedHardLinks("hardlink".into()),
        ),
        (
            "dir",
            DIR,
            2,
            &b"data"[..],
            ArchiveError::InvalidEntry("dir".into()),
        ),
    ] {
        assert_eq!(
            CpioFs::from_bytes(archive(&[(name, mode, nlink, data)])).err(),
            Some(error)
        );
    }
}

#[test]
fn rejects_duplicates_and_conflicts_in_either_order() {
    assert_eq!(
        CpioFs::from_bytes(archive(&[("a", FILE, 1, b""), ("./a", FILE, 1, b"")])).err(),
        Some(ArchiveError::DuplicatePath("./a".into()))
    );
    let file = ("a", FILE, 1, &b""[..]);
    let child = ("a/b", FILE, 1, &b""[..]);
    for (entries, name) in [([file, child], "a/b"), ([child, file], "a")] {
        assert_eq!(
            CpioFs::from_bytes(archive(&entries)).err(),
            Some(ArchiveError::PathConflict(name.into()))
        );
    }
}

#[test]
fn empty_archive_has_an_empty_root() {
    let fs = CpioFs::from_bytes(archive(&[])).unwrap();
    assert!(fs.read_dir(fs.root()).unwrap().is_empty());
    assert!(matches!(
        CpioFs::from_bytes(Vec::new()),
        Err(ArchiveError::Parse(_))
    ));
}
