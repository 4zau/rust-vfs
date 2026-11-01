use std::print;

use memfs::MemFs;
use vfs_core::{FileSystem, NodeKind, Vfs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // setup root memfs
    let mut root_fs = MemFs::new();
    let hello = root_fs.create(root_fs.root(), "hello.txt", NodeKind::File)?;
    root_fs.write(hello, 0, b"hello from root!")?;

    let data = root_fs.create(root_fs.root(), "data", NodeKind::Directory)?;
    let hidden = root_fs.create(data, "root-only.txt", NodeKind::File)?;
    root_fs.write(hidden, 0, b"this file belongs to the root filesystem.")?;

    // another one
    let mut data_fs = MemFs::new();
    let config = data_fs.create(data_fs.root(), "config.txt", NodeKind::File)?;
    data_fs.write(config, 0, b"mode=demo\n")?;

    let mut vfs = Vfs::new();
    let root_id = vfs.register("/", Box::new(root_fs))?;
    let data_id = vfs.register("/data", Box::new(data_fs))?;
    println!("registered FS: / = {root_id:?}, /data = {data_id:?}");

    for path in [
        "/",
        "/hello.txt",
        "/data",
        "/data/config.txt",
        "//data/./config.txt",
        "/data/../hello.txt",
    ] {
        let metadata = vfs.metadata(path)?;
        println!("{path}: {:?}, {} bytes", metadata.kind, metadata.size);
    }

    println!("hidden file: {:?}", vfs.metadata("/data/root-only.txt"));
    println!("file with trailing /: {:?}", vfs.metadata("/hello.txt/"));

    vfs.unregister(data_id)?;
    println!("after unregistering /data:");
    println!(
        "/data/root-only.txt: {:?}",
        vfs.metadata("/data/root-only.txt")?
    );
    println!("/data/config.txt: {:?}", vfs.metadata("/data/config.txt"));

    vfs.unregister(root_id)?;
    println!("after unregistering /: {:?}", vfs.metadata("/"));

    print!("\n\nwow, isn't this cool.\n");
    Ok(())
}
