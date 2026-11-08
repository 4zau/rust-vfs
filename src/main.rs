use std::{
    io::{self, Write},
    process::ExitCode,
};

use cpiofs::CpioFs;
use memfs::MemFs;
use vfs_core::{FileSystem, MetadataUpdate, NodeKind, Vfs};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let Some(archive) = args.next() else {
        return demo_memfs();
    };

    let path = args.next();
    if args.next().is_some() {
        return Err("usage: rust-vfs <archive.cpio> [/initrd/path]".into());
    }

    let path = path.as_deref().unwrap_or(std::ffi::OsStr::new("/initrd"));
    let path = path.to_str().ok_or("VFS paths must be UTF-8")?;

    let bytes = std::fs::read(&archive).map_err(|error| format!("read {archive:?}: {error}"))?;
    let fs = CpioFs::from_bytes(bytes).map_err(|error| format!("load {archive:?}: {error}"))?;

    let mut vfs = Vfs::new();
    vfs.register("/initrd", Box::new(fs))
        .map_err(|error| format!("register {archive:?} at /initrd: {error}"))?;
    for (id, prefix) in vfs.registered_prefixes() {
        eprintln!("{id:?} {prefix}");
    }

    let metadata = vfs
        .metadata(path)
        .map_err(|error| format!("get metadata for {path:?}: {error}"))?;

    match metadata.kind {
        NodeKind::Directory => {
            for entry in vfs
                .read_dir(path)
                .map_err(|error| format!("list {path:?}: {error}"))?
            {
                println!("{:?} {}", entry.kind, entry.name);
            }
        }
        NodeKind::File => {
            let fd = vfs
                .open(path)
                .map_err(|error| format!("open {path:?}: {error}"))?;
            let mut output = io::stdout().lock();
            let mut buffer = [0; 4096];
            loop {
                let count = vfs
                    .read(fd, &mut buffer)
                    .map_err(|error| format!("read {path:?}: {error}"))?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count])?;
            }
            vfs.close(fd)
                .map_err(|error| format!("close {path:?}: {error}"))?;
            output.flush()?;
        }
    }
    Ok(())
}

fn demo_memfs() -> Result<(), Box<dyn std::error::Error>> {
    let mut vfs = Vfs::new();

    let mut app_fs = MemFs::default();
    let file = app_fs.create(app_fs.root(), "run.sh", NodeKind::File)?;
    app_fs.write(file, 0, b"#!/bin/sh\necho Hello!")?;

    let app_id = vfs.register("/app", Box::new(app_fs))?;
    println!(">>> Смонтировали ФС /app (ID: {app_id:?})\n");

    let meta = vfs.metadata("/app/run.sh")?;
    println!("--- 1. Исходные метаданные /app/run.sh ---");
    println!("Тип:      {:?}", meta.kind);
    println!("Размер:   {} байт", meta.size);
    println!("Права:    0o{:06o} (rw-r--r--)", meta.mode);
    println!("Владелец: UID={}, GID={}", meta.uid, meta.gid);
    println!("Время:    {} сек\n", meta.mtime);

    println!("--- 2. Меняем метаданные через Vfs::set_metadata ---");
    vfs.set_metadata(
        "/app/run.sh",
        MetadataUpdate {
            permissions: Some(0o755),
            uid: Some(1000),
            gid: Some(1000),
            mtime: Some(1_700_000_000),
            ..MetadataUpdate::default()
        },
    )?;
    println!("Метаданные успешно обновлены через VFS!\n");

    let updated = vfs.metadata("/app/run.sh")?;
    println!("--- 3. Метаданные после изменения ---");
    println!("Тип:      {:?}", updated.kind);
    println!("Размер:   {} байт", updated.size);
    println!(
        "Права:    0o{:06o} (rwxr-xr-x) <-- ИЗМЕНИЛИСЬ",
        updated.mode
    );
    println!(
        "Владелец: UID={}, GID={}       <-- ИЗМЕНИЛИСЬ",
        updated.uid, updated.gid
    );
    println!("Время:    {} сек         <-- ИЗМЕНИЛИСЬ\n", updated.mtime);

    Ok(())
}
