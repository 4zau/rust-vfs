use memfs::MemFs;
use vfs_core::{FileSystem, FsError, NodeKind, Vfs, VfsError};

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
fn parent_components_are_normalized_before_routing() {
    let mut vfs = Vfs::new();
    vfs.register("/", Box::new(filesystem("file", 5))).unwrap();
    vfs.register("/data", Box::new(filesystem("file", 9)))
        .unwrap();
    for path in [
        "/data/../file",
        "/../../file",
        "/missing/../file",
        "/каталог/../file",
    ] {
        assert_eq!(vfs.metadata(path).unwrap().size, 5);
    }
    assert_eq!(vfs.metadata("/data/missing/../file").unwrap().size, 9);
    for path in ["/data/..", "/..", "/../../", "/data/../."] {
        assert_eq!(vfs.metadata(path).unwrap().kind, NodeKind::Directory);
    }
    for path in ["/file/.", "/file/missing/..", "/file/missing/../"] {
        assert_eq!(vfs.metadata(path), Err(VfsError::Fs(FsError::NotDirectory)));
    }
    assert_eq!(
        vfs.register("/data/cache/..", Box::new(MemFs::new())),
        Err(VfsError::PrefixAlreadyRegistered)
    );
}
