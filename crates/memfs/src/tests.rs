use super::*;
use alloc::vec;

fn contents(fs: &MemFs, file: NodeId) -> Vec<u8> {
    let mut bytes = vec![0; fs.metadata(file).unwrap().size];
    assert_eq!(fs.read(file, 0, &mut bytes), Ok(bytes.len()));
    bytes
}

#[test]
fn creates_nested_nodes_and_lists_sorted_entries() {
    let mut fs = MemFs::new();
    let root = fs.root();
    assert_eq!(root, NodeId::new(0));
    assert_eq!(
        fs.metadata(root),
        Ok(Metadata {
            kind: NodeKind::Directory,
            size: 0,
            ..Metadata::new(NodeKind::Directory)
        })
    );
    assert_eq!(fs.read_dir(root), Ok(vec![]));
    let dir = fs.create(root, "z-dir", NodeKind::Directory).unwrap();
    let file = fs.create(root, "a-file", NodeKind::File).unwrap();
    let child = fs.create(dir, "child", NodeKind::File).unwrap();
    assert_eq!(fs.lookup(dir, "child"), Ok(child));
    assert_ne!(file, child);
    assert_eq!(
        fs.metadata(file),
        Ok(Metadata {
            kind: NodeKind::File,
            size: 0,
            ..Metadata::new(NodeKind::File)
        })
    );
    assert_eq!(
        fs.read_dir(root),
        Ok(vec![
            DirEntry {
                name: String::from("a-file"),
                node: file,
                kind: NodeKind::File
            },
            DirEntry {
                name: String::from("z-dir"),
                node: dir,
                kind: NodeKind::Directory
            },
        ])
    );
    assert_eq!(
        fs.create(root, "a-file", NodeKind::Directory),
        Err(FsError::AlreadyExists)
    );
    assert_eq!(fs.lookup(root, "a-file"), Ok(file));
}

#[test]
fn writes_overwrite_and_fill_gaps_without_affecting_other_files() {
    let mut fs = MemFs::new();
    let file = fs.create(fs.root(), "file", NodeKind::File).unwrap();
    let other = fs.create(fs.root(), "other", NodeKind::File).unwrap();
    assert_eq!(fs.write(file, 0, b"hello"), Ok(5));
    assert_eq!(fs.write(file, 1, b"XY"), Ok(2));
    assert_eq!(fs.write(file, 7, b"!"), Ok(1));
    assert_eq!(contents(&fs, file), b"hXYlo\0\0!");
    assert_eq!(contents(&fs, other), b"");
    assert_eq!(fs.write(file, usize::MAX, b""), Ok(0));
    assert_eq!(fs.metadata(file).unwrap().size, 8);
}

#[test]
fn reads_partial_buffers_and_handles_eof() {
    let mut fs = MemFs::new();
    let file = fs.create(fs.root(), "file", NodeKind::File).unwrap();
    fs.write(file, 0, b"abcde").unwrap();
    let mut buffer = [9; 4];
    assert_eq!(fs.read(file, 1, &mut buffer), Ok(4));
    assert_eq!(&buffer, b"bcde");
    buffer.fill(9);
    assert_eq!(fs.read(file, 3, &mut buffer), Ok(2));
    assert_eq!(buffer, [b'd', b'e', 9, 9]);
    for offset in [5, 6, usize::MAX] {
        assert_eq!(fs.read(file, offset, &mut buffer), Ok(0));
        assert_eq!(buffer, [b'd', b'e', 9, 9]);
    }
    assert_eq!(fs.read(file, 0, &mut []), Ok(0));
}

#[test]
fn truncation_discards_bytes_and_zero_fills_growth() {
    let mut fs = MemFs::new();
    let file = fs.create(fs.root(), "file", NodeKind::File).unwrap();
    fs.write(file, 0, b"abcdef").unwrap();
    fs.truncate(file, 3).unwrap();
    assert_eq!(contents(&fs, file), b"abc");
    fs.truncate(file, 6).unwrap();
    assert_eq!(contents(&fs, file), b"abc\0\0\0");
    fs.truncate(file, 6).unwrap();
    assert_eq!(contents(&fs, file), b"abc\0\0\0");
    fs.truncate(file, 0).unwrap();
    assert_eq!(contents(&fs, file), b"");
}

#[test]
fn invalid_names_do_not_mutate_the_directory() {
    let mut fs = MemFs::new();
    for name in ["", ".", "..", "/", "a/b"] {
        assert_eq!(
            fs.create(fs.root(), name, NodeKind::File),
            Err(FsError::InvalidName)
        );
        assert_eq!(fs.lookup(fs.root(), name), Err(FsError::InvalidName));
        assert_eq!(fs.remove(fs.root(), name), Err(FsError::InvalidName));
    }
    assert_eq!(fs.read_dir(fs.root()), Ok(vec![]));
    let file = fs.create(fs.root(), "данные.txt", NodeKind::File).unwrap();
    assert_eq!(fs.lookup(fs.root(), "данные.txt"), Ok(file));
}

#[test]
fn missing_nodes_and_wrong_node_types_return_errors() {
    let mut fs = MemFs::new();
    let root = fs.root();
    let file = fs.create(root, "file", NodeKind::File).unwrap();
    let missing = NodeId::new(999);
    assert_eq!(fs.metadata(missing), Err(FsError::NotFound));
    assert_eq!(fs.lookup(root, "missing"), Err(FsError::NotFound));
    assert_eq!(fs.remove(root, "missing"), Err(FsError::NotFound));
    for (node, error) in [(file, FsError::NotDirectory), (missing, FsError::NotFound)] {
        assert_eq!(fs.lookup(node, "child"), Err(error));
        assert_eq!(fs.read_dir(node), Err(error));
        assert_eq!(fs.create(node, "child", NodeKind::File), Err(error));
        assert_eq!(fs.remove(node, "child"), Err(error));
    }
    for (node, error) in [(root, FsError::IsDirectory), (missing, FsError::NotFound)] {
        assert_eq!(fs.read(node, 0, &mut [0; 1]), Err(error));
        assert_eq!(fs.write(node, 0, b"x"), Err(error));
        assert_eq!(fs.write(node, 0, b""), Err(error));
        assert_eq!(fs.truncate(node, 0), Err(error));
    }
}

#[test]
fn removal_rejects_nonempty_directories_and_invalidates_ids() {
    let mut fs = MemFs::new();
    let root = fs.root();
    let dir = fs.create(root, "dir", NodeKind::Directory).unwrap();
    let file = fs.create(dir, "file", NodeKind::File).unwrap();
    fs.write(file, 0, b"saved").unwrap();
    assert_eq!(fs.remove(root, "dir"), Err(FsError::DirectoryNotEmpty));
    assert_eq!(contents(&fs, file), b"saved");
    fs.remove(dir, "file").unwrap();
    assert_eq!(fs.metadata(file), Err(FsError::NotFound));
    assert_eq!(fs.read(file, 0, &mut [0; 1]), Err(FsError::NotFound));
    assert_eq!(fs.write(file, 0, b"x"), Err(FsError::NotFound));
    assert_eq!(fs.truncate(file, 0), Err(FsError::NotFound));
    assert_eq!(fs.lookup(dir, "file"), Err(FsError::NotFound));
    let replacement = fs.create(dir, "file", NodeKind::File).unwrap();
    assert_ne!(replacement, file);
    fs.remove(dir, "file").unwrap();
    fs.remove(root, "dir").unwrap();
    assert_eq!(fs.metadata(dir), Err(FsError::NotFound));
    assert_eq!(fs.read_dir(root), Ok(vec![]));
    assert!(fs.metadata(root).is_ok());
}

#[test]
fn overflowing_sizes_preserve_existing_contents() {
    let mut fs = MemFs::new();
    let file = fs.create(fs.root(), "file", NodeKind::File).unwrap();
    fs.write(file, 0, b"saved").unwrap();
    assert_eq!(fs.write(file, usize::MAX, b"x"), Err(FsError::SizeOverflow));
    assert_eq!(
        fs.write(file, isize::MAX as usize, b"x"),
        Err(FsError::SizeOverflow)
    );
    assert_eq!(
        fs.truncate(file, isize::MAX as usize + 1),
        Err(FsError::SizeOverflow)
    );
    assert_eq!(contents(&fs, file), b"saved");
}

#[test]
fn last_identifier_is_allocated_once_without_wrapping() {
    let mut fs = MemFs::new();
    fs.next_id = Some(u64::MAX);
    let root = fs.root();
    let last = fs.create(root, "last", NodeKind::File).unwrap();
    assert_eq!(last, NodeId::new(u64::MAX));
    assert_eq!(
        fs.create(root, "extra", NodeKind::File),
        Err(FsError::IdentifierExhausted)
    );
    assert_eq!(fs.lookup(root, "extra"), Err(FsError::NotFound));
    assert_eq!(fs.read_dir(root).unwrap().len(), 1);
    fs.remove(root, "last").unwrap();
    assert_eq!(
        fs.create(root, "extra", NodeKind::File),
        Err(FsError::IdentifierExhausted)
    );
    assert_eq!(fs.read_dir(root), Ok(vec![]));
}

#[test]
fn stored_metadata_survives_content_changes_and_vfs_registration() {
    let mut fs = MemFs::new();
    let root = fs.root();
    let file = fs.create(root, "file", NodeKind::File).unwrap();
    let other = fs.create(root, "other", NodeKind::File).unwrap();
    let metadata = Metadata {
        mode: 0o104750,
        uid: 1000,
        gid: 1001,
        nlink: 3,
        mtime: 1_700_000_000,
        ..Metadata::new(NodeKind::File)
    };
    fs.set_metadata(file, metadata).unwrap();
    fs.write(file, 0, b"hello").unwrap();
    assert_eq!(
        fs.metadata(file).unwrap(),
        Metadata {
            size: 5,
            ..metadata
        }
    );
    fs.truncate(file, 2).unwrap();
    assert_eq!(
        fs.metadata(file).unwrap(),
        Metadata {
            size: 2,
            ..metadata
        }
    );
    assert_eq!(fs.metadata(other).unwrap(), Metadata::new(NodeKind::File));

    let directory = Metadata {
        mode: 0o041770,
        uid: 42,
        mtime: 1234,
        ..Metadata::new(NodeKind::Directory)
    };
    fs.set_metadata(root, directory).unwrap();
    assert_eq!(fs.metadata(root).unwrap(), directory);

    let mut vfs = vfs_core::Vfs::new();
    vfs.register("/mem", alloc::boxed::Box::new(fs)).unwrap();
    assert_eq!(vfs.metadata("/mem").unwrap(), directory);
    assert_eq!(
        vfs.metadata("/mem/file").unwrap(),
        Metadata {
            size: 2,
            ..metadata
        }
    );
}

#[test]
fn invalid_metadata_is_rejected_without_changing_the_node() {
    let mut fs = MemFs::new();
    let root = fs.root();
    let file = fs.create(root, "file", NodeKind::File).unwrap();
    fs.write(file, 0, b"saved").unwrap();
    let original = fs.metadata(file).unwrap();
    for invalid in [
        Metadata {
            kind: NodeKind::Directory,
            ..original
        },
        Metadata {
            size: 0,
            ..original
        },
        Metadata {
            mode: 0o040755,
            ..original
        },
        Metadata {
            mode: 0o120777,
            ..original
        },
    ] {
        assert_eq!(
            fs.set_metadata(file, invalid),
            Err(FsError::InvalidMetadata)
        );
        assert_eq!(fs.metadata(file).unwrap(), original);
        assert_eq!(contents(&fs, file), b"saved");
    }
    let directory = fs.metadata(root).unwrap();
    assert_eq!(
        fs.set_metadata(
            root,
            Metadata {
                size: 1,
                ..directory
            }
        ),
        Err(FsError::InvalidMetadata)
    );
    assert_eq!(fs.metadata(root).unwrap(), directory);
    assert_eq!(
        fs.set_metadata(NodeId::new(999), original),
        Err(FsError::NotFound)
    );
    fs.remove(root, "file").unwrap();
    assert_eq!(fs.set_metadata(file, original), Err(FsError::NotFound));
}
