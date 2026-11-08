use memfs::MemFs;
use vfs_core::{FileSystem, FsError, MetadataUpdate, NodeKind, Vfs, VfsError};

fn filesystem(name: &str, size: usize) -> MemFs {
    let mut fs = MemFs::new();
    let node = fs.create(fs.root(), name, NodeKind::File).unwrap();
    fs.truncate(node, size).unwrap();
    fs
}

#[test]
fn routes_independent_instances_and_normalizes_paths() {
    let mut vfs = Vfs::new();
    vfs.register("/data/", Box::new(filesystem("file", 3)))
        .unwrap();
    vfs.register("/other", Box::new(filesystem("file", 7)))
        .unwrap();
    assert_eq!(vfs.metadata("//data/./file").unwrap().size, 3);
    assert_eq!(vfs.metadata("/other/file").unwrap().size, 7);
    assert_eq!(
        vfs.metadata("/database/file"),
        Err(VfsError::Fs(FsError::NotFound))
    );
    assert_eq!(
        vfs.metadata("/data/file/"),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    assert_eq!(
        vfs.metadata("/data/file/child"),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    for path in ["", "data", "/a\0b"] {
        assert_eq!(vfs.metadata(path), Err(VfsError::InvalidPath));
    }
}

#[test]
fn nested_mount_has_priority_without_fallback() {
    let mut root = MemFs::new();
    let data = root
        .create(root.root(), "data", NodeKind::Directory)
        .unwrap();
    root.create(data, "hidden", NodeKind::File).unwrap();
    let mut vfs = Vfs::new();
    let parent = vfs.register("/", Box::new(root)).unwrap();
    let child = vfs
        .register("/data", Box::new(filesystem("file", 4)))
        .unwrap();
    assert_eq!(vfs.metadata("/data/file").unwrap().size, 4);
    assert_eq!(
        vfs.metadata("/data/hidden"),
        Err(VfsError::Fs(FsError::NotFound))
    );
    assert_eq!(
        vfs.register("//data/./", Box::new(MemFs::new())),
        Err(VfsError::PrefixAlreadyRegistered)
    );
    assert_eq!(vfs.metadata("/data/file").unwrap().size, 4);
    vfs.unregister(parent).unwrap();
    assert_eq!(vfs.metadata("/data/file").unwrap().size, 4);
    vfs.unregister(child).unwrap();
    assert_eq!(vfs.unregister(child), Err(VfsError::UnknownFileSystem));
    let replacement = vfs.register("/data", Box::new(MemFs::new())).unwrap();
    assert_ne!(replacement, child);
}

#[test]
fn mount_prefixes_do_not_create_virtual_ancestors() {
    let mut vfs = Vfs::new();
    let not_found = Err(VfsError::Fs(FsError::NotFound));
    assert_eq!(vfs.metadata("/"), not_found);
    let child = vfs
        .register("/data/cache", Box::new(filesystem("file", 3)))
        .unwrap();
    assert_eq!(vfs.metadata("/"), not_found);
    assert_eq!(vfs.metadata("/data"), not_found);
    assert_eq!(
        vfs.metadata("/data/cache").unwrap().kind,
        NodeKind::Directory
    );
    assert_eq!(vfs.metadata("/data/cache/file").unwrap().size, 3);

    let root = vfs.register("/", Box::new(filesystem("data", 5))).unwrap();
    assert_eq!(vfs.metadata("/").unwrap().kind, NodeKind::Directory);
    // Вложенное подключение не подменяет реальный файл-предок каталогом.
    assert_eq!(vfs.metadata("/data").unwrap().kind, NodeKind::File);
    assert_eq!(
        vfs.metadata("/data/"),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    // Выбор подключения выполняется до обхода внутренних узлов.
    assert_eq!(vfs.metadata("/data/cache/file").unwrap().size, 3);
    vfs.unregister(root).unwrap();
    assert_eq!(vfs.metadata("/data"), not_found);
    assert_eq!(vfs.metadata("/data/cache/file").unwrap().size, 3);
    vfs.unregister(child).unwrap();
    assert_eq!(vfs.metadata("/data/cache"), not_found);
}

#[test]
fn parent_components_are_rejected_before_routing() {
    let mut vfs = Vfs::new();
    vfs.register("/", Box::new(filesystem("file", 5))).unwrap();
    for path in [
        "/..",
        "/../../file",
        "/data/../file",
        "/missing/../file",
        "/каталог/../file",
        "/file/..",
        "/file/../",
    ] {
        assert_eq!(vfs.metadata(path), Err(VfsError::InvalidPath));
        assert_eq!(vfs.open(path), Err(VfsError::InvalidPath));
        assert_eq!(
            vfs.register(path, Box::new(MemFs::new())),
            Err(VfsError::InvalidPath)
        );
    }
    assert_eq!(vfs.metadata("/file").unwrap().size, 5);
    assert_eq!(
        vfs.metadata("/file/."),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    vfs.register("/dots", Box::new(filesystem("..file", 2)))
        .unwrap();
    assert_eq!(vfs.metadata("/dots/..file").unwrap().size, 2);
}
#[test]
fn open_files_have_independent_offsets_and_reach_eof() {
    let mut fs = MemFs::new();
    let node = fs.create(fs.root(), "file", NodeKind::File).unwrap();
    fs.write(node, 0, b"abcdef").unwrap();
    let mut vfs = Vfs::new();
    vfs.register("/data", Box::new(fs)).unwrap();
    let first = vfs.open("/data/file").unwrap();
    let second = vfs.open("/data/file").unwrap();
    assert_ne!(first, second);
    let mut buffer = [0xcc; 4];
    assert_eq!(vfs.read(first, &mut []), Ok(0));
    assert_eq!(vfs.read(first, &mut buffer), Ok(4));
    assert_eq!(&buffer, b"abcd");
    buffer.fill(0xcc);
    assert_eq!(vfs.read(first, &mut buffer), Ok(2));
    assert_eq!(&buffer, &[b'e', b'f', 0xcc, 0xcc]);
    assert_eq!(vfs.read(first, &mut buffer), Ok(0));
    assert_eq!(vfs.read(first, &mut buffer), Ok(0));
    assert_eq!(vfs.read(second, &mut buffer), Ok(4));
    assert_eq!(&buffer, b"abcd");
    vfs.close(first).unwrap();
    assert_eq!(vfs.close(first), Err(VfsError::InvalidDescriptor));
    assert_eq!(vfs.read(first, &mut []), Err(VfsError::InvalidDescriptor));
    assert_eq!(
        vfs.read(first, &mut buffer),
        Err(VfsError::InvalidDescriptor)
    );
    let third = vfs.open("/data/file").unwrap();
    assert_ne!(first, third);
    assert_eq!(vfs.read(third, &mut buffer), Ok(4));
    assert_eq!(&buffer, b"abcd");
}

#[test]
fn unregister_is_busy_until_all_its_files_are_closed() {
    let mut vfs = Vfs::new();
    let parent = vfs.register("/", Box::new(filesystem("file", 2))).unwrap();
    let child = vfs
        .register("/data", Box::new(filesystem("file", 3)))
        .unwrap();
    let first = vfs.open("/data/file").unwrap();
    let second = vfs.open("/data/file").unwrap();
    assert_eq!(vfs.unregister(child), Err(VfsError::Busy));
    // Открытия дочерней ФС не блокируют удаление родительского подключения.
    vfs.unregister(parent).unwrap();
    assert_eq!(vfs.metadata("/data/file").unwrap().size, 3);
    vfs.close(first).unwrap();
    assert_eq!(vfs.unregister(child), Err(VfsError::Busy));
    vfs.close(second).unwrap();
    vfs.unregister(child).unwrap();
}

#[test]
fn open_rejects_directories_and_bad_paths_without_blocking_unregister() {
    let mut vfs = Vfs::new();
    let id = vfs
        .register("/data", Box::new(filesystem("file", 3)))
        .unwrap();
    assert_eq!(vfs.open("/data"), Err(VfsError::Fs(FsError::IsDirectory)));
    assert_eq!(
        vfs.open("/data/file/"),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    assert_eq!(
        vfs.open("/data/file/."),
        Err(VfsError::Fs(FsError::NotDirectory))
    );
    assert_eq!(
        vfs.open("/data/missing"),
        Err(VfsError::Fs(FsError::NotFound))
    );
    assert_eq!(vfs.open("relative"), Err(VfsError::InvalidPath));
    vfs.unregister(id).unwrap();
}

#[test]
fn mounting_over_a_path_does_not_redirect_an_open_file() {
    let mut vfs = Vfs::new();
    let root = vfs.register("/", Box::new(filesystem("file", 5))).unwrap();
    let old = vfs.open("/file").unwrap();
    let overlay = vfs
        .register("/file", Box::new(filesystem("file", 9)))
        .unwrap();
    assert_eq!(vfs.open("/file"), Err(VfsError::Fs(FsError::IsDirectory)));
    let new = vfs.open("/file/file").unwrap();
    let mut buffer = [0xff; 16];
    assert_eq!(vfs.read(old, &mut buffer), Ok(5));
    assert_eq!(vfs.read(new, &mut buffer), Ok(9));
    assert_eq!(vfs.unregister(root), Err(VfsError::Busy));
    vfs.close(old).unwrap();
    vfs.unregister(root).unwrap();
    assert_eq!(vfs.unregister(overlay), Err(VfsError::Busy));
    vfs.close(new).unwrap();
    vfs.unregister(overlay).unwrap();
}

#[test]
fn set_metadata_checks_directory_suffix_against_the_resolved_node() {
    let mut vfs = Vfs::new();
    vfs.register("/data", Box::new(filesystem("file", 3)))
        .unwrap();

    let mut expected_file = vfs.metadata("/data/file").unwrap();
    expected_file.uid = 1000;
    let update = MetadataUpdate {
        uid: Some(1000),
        ..MetadataUpdate::default()
    };
    vfs.set_metadata("//data/./file", update).unwrap();
    assert_eq!(vfs.metadata("/data/file"), Ok(expected_file));

    let mut expected_directory = vfs.metadata("/data").unwrap();
    expected_directory.gid = 2000;
    let update = MetadataUpdate {
        gid: Some(2000),
        ..MetadataUpdate::default()
    };
    vfs.set_metadata("/data/", update).unwrap();
    assert_eq!(vfs.metadata("/data"), Ok(expected_directory));

    for path in ["/data/file/", "/data/file/."] {
        assert_eq!(
            vfs.set_metadata(path, update),
            Err(VfsError::Fs(FsError::NotDirectory))
        );
    }
    assert_eq!(vfs.metadata("/data/file"), Ok(expected_file));
    assert_eq!(vfs.metadata("/data"), Ok(expected_directory));
}
