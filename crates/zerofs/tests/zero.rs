use vfs_core::{FileSystem, FsError, NodeId, NodeKind, Vfs};
use zerofs::ZeroFs;

#[test]
fn root_contains_only_zero() {
    let fs = ZeroFs::new();
    let entries = fs.read_dir(fs.root()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "zero");
    assert_eq!(entries[0].kind, NodeKind::File);
    let zero = fs.lookup(fs.root(), "zero").unwrap();
    assert_eq!(entries[0].node, zero);
    let metadata = fs.metadata(zero).unwrap();
    assert_eq!(metadata.kind, NodeKind::File);
    assert_eq!(metadata.size, 0);
    assert_eq!(metadata.mode, 0o100666);
    assert_eq!(fs.metadata(fs.root()).unwrap().kind, NodeKind::Directory);
}

#[test]
fn reads_fill_buffers_at_any_offset_and_writes_are_discarded() {
    let mut fs = ZeroFs::new();
    let zero = fs.lookup(fs.root(), "zero").unwrap();
    for offset in [0, 17, usize::MAX] {
        assert_eq!(fs.write(zero, offset, b"discard me"), Ok(10));
        let mut buffer = [0xff; 31];
        assert_eq!(fs.read(zero, offset, &mut buffer), Ok(31));
        assert_eq!(buffer, [0; 31]);
    }
    assert_eq!(fs.read(zero, usize::MAX, &mut []), Ok(0));
    assert_eq!(fs.write(zero, usize::MAX, &[]), Ok(0));
    assert_eq!(fs.metadata(zero).unwrap().size, 0);
}

#[test]
fn invalid_nodes_and_node_kinds_are_rejected_without_changing_buffers() {
    let mut fs = ZeroFs::new();
    let zero = fs.lookup(fs.root(), "zero").unwrap();
    let missing = NodeId::new(42);
    assert_eq!(fs.metadata(missing), Err(FsError::NotFound));
    assert_eq!(fs.read_dir(zero), Err(FsError::NotDirectory));
    assert_eq!(fs.read_dir(missing), Err(FsError::NotFound));
    assert_eq!(fs.lookup(zero, "zero"), Err(FsError::NotDirectory));
    assert_eq!(fs.lookup(missing, "zero"), Err(FsError::NotFound));
    assert_eq!(fs.lookup(fs.root(), "other"), Err(FsError::NotFound));
    for name in ["", ".", "..", "a/b", "a\0b"] {
        assert_eq!(fs.lookup(fs.root(), name), Err(FsError::InvalidName));
    }
    for (node, error) in [
        (fs.root(), FsError::IsDirectory),
        (missing, FsError::NotFound),
    ] {
        let mut buffer = [0xff; 3];
        assert_eq!(fs.read(node, 0, &mut buffer), Err(error));
        assert_eq!(buffer, [0xff; 3]);
        assert_eq!(fs.write(node, 0, b"data"), Err(error));
    }
    assert_eq!(fs.truncate(zero, 1), Err(FsError::Unsupported));
    assert_eq!(fs.remove(fs.root(), "zero"), Err(FsError::Unsupported));
}

#[test]
fn vfs_reads_continue_despite_zero_metadata_size() {
    let mut vfs = Vfs::new();
    vfs.register("/dev", Box::new(ZeroFs::new())).unwrap();
    assert_eq!(vfs.metadata("/dev/zero").unwrap().size, 0);
    let fd = vfs.open("/dev/zero").unwrap();
    for _ in 0..3 {
        let mut buffer = [0xff; 256];
        assert_eq!(vfs.read(fd, &mut buffer).unwrap(), buffer.len());
        assert_eq!(buffer, [0; 256]);
    }
    vfs.close(fd).unwrap();
}
