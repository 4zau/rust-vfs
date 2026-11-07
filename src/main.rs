use memfs::MemFs;
use vfs_core::{FileSystem, NodeKind, Vfs};

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
    let mut new_meta = meta;
    new_meta.mode = 0o100755;
    new_meta.uid = 1000;
    new_meta.gid = 1000;
    new_meta.mtime = 1_700_000_000;

    vfs.set_metadata("/app/run.sh", new_meta)?;
    println!("Метаданные успешно обновлены через VFS!\n");

    let updated = vfs.metadata("/app/run.sh")?;
    println!("--- 3. Метаданные после изменения ---");
    println!("Тип:      {:?}", updated.kind);
    println!("Размер:   {} байт", updated.size);
    println!("Права:    0o{:06o} (rwxr-xr-x) <-- ИЗМЕНИЛИСЬ", updated.mode);
    println!("Владелец: UID={}, GID={}       <-- ИЗМЕНИЛИСЬ", updated.uid, updated.gid);
    println!("Время:    {} сек         <-- ИЗМЕНИЛИСЬ\n", updated.mtime);

    println!("--- 4. Проверка защиты от некорректных метаданных ---");
    let mut hacked_meta = updated;
    hacked_meta.size = 9999; 
    let result = vfs.set_metadata("/app/run.sh", hacked_meta);
    println!(
        "Попытка подделать размер файла завершилась с ошибкой: {:?}", 
        result.err().unwrap()
    );

    Ok(())
}