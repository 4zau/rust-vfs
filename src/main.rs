use memfs::MemFs;
use vfs_core::{FileSystem, MetadataUpdate, NodeKind, Vfs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
