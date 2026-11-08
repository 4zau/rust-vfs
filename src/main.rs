use std::io::{self, Write};
use memfs::MemFs;
use vfs_core::{FileSystem, NodeKind, Vfs};

/// Нормализует путь относительно текущей директории cwd (поддерживает ".", "..", относительные пути)
fn resolve_shell_path(cwd: &str, input: &str) -> String {
    let input = input.trim();
    let raw = if input.starts_with('/') {
        input.to_string()
    } else if cwd == "/" {
        format!("/{}", input)
    } else {
        format!("{}/{}", cwd, input)
    };

    let mut components = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                components.pop();
            }
            c => components.push(c),
        }
    }

    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut vfs = Vfs::new();

    // 1. Создаем корневую ФС и наполняем ее тестовыми файлами и папками
    let mut root_fs = MemFs::default();
    let root = root_fs.root();

    let hello = root_fs.create(root, "hello.txt", NodeKind::File)?;
    root_fs.write(hello, 0, b"Hello from root filesystem!\nWelcome to Rust VFS shell.")?;

    let docs = root_fs.create(root, "docs", NodeKind::Directory)?;
    let readme = root_fs.create(docs, "readme.txt", NodeKind::File)?;
    root_fs.write(readme, 0, b"Rust VFS project demonstration.\nCommands: ls, cd, cat, stat, pwd, help, exit")?;

    let bin = root_fs.create(root, "bin", NodeKind::Directory)?;
    let script = root_fs.create(bin, "run.sh", NodeKind::File)?;
    root_fs.write(script, 0, b"#!/bin/sh\necho 'VFS script executed!'")?;

    // Создаем директорию для точки монтирования
    root_fs.create(root, "data", NodeKind::Directory)?;

    // 2. Вторая ФС, смонтированная по пути /data
    let mut data_fs = MemFs::default();
    let data_root = data_fs.root();
    let conf = data_fs.create(data_root, "config.json", NodeKind::File)?;
    data_fs.write(conf, 0, b"{\n  \"env\": \"dev\",\n  \"mounted\": true\n}")?;

    let notes = data_fs.create(data_root, "notes.txt", NodeKind::File)?;
    data_fs.write(notes, 0, b"This filesystem is mounted under /data prefix.")?;

    // Регистрируем ФС в VFS
    vfs.register("/", Box::new(root_fs))?;
    vfs.register("/data", Box::new(data_fs))?;

    println!("=====================================================");
    println!("  Добро пожаловать в интерактивный shell Rust VFS!   ");
    println!("  Смонтированы ФС: '/' и '/data'                     ");
    println!("  Введите 'help' для списка доступных команд.        ");
    println!("=====================================================\n");

    let mut cwd = String::from("/");
    let stdin = io::stdin();

    loop {
        // Выводим приглашение ввода, например: vfs:/docs>
        print!("vfs:{cwd}> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break; // EOF (Ctrl+D)
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "help" => {
                println!("Доступные команды:");
                println!("  ls [путь]     - список файлов каталога (по умолчанию в cwd)");
                println!("  cd <путь>     - сменить каталог (поддерживает .., относительные пути)");
                println!("  pwd           - показать текущий рабочий каталог");
                println!("  cat <файл>    - вывести содержимое файла");
                println!("  stat <путь>   - показать полные метаданные узла");
                println!("  clear         - очистить экран терминала");
                println!("  exit / quit   - выйти из shell");
            }

            "pwd" => {
                println!("{cwd}");
            }

            "cd" => {
                let target = if args.is_empty() {
                    "/".to_string()
                } else {
                    resolve_shell_path(&cwd, args[0])
                };

                match vfs.metadata(&target) {
                    Ok(meta) => {
                        if meta.kind == NodeKind::Directory {
                            cwd = target;
                        } else {
                            println!("cd: не является каталогом: {}", args[0]);
                        }
                    }
                    Err(e) => println!("cd: {}: {}", args[0], e),
                }
            }

            "ls" => {
                let target = if args.is_empty() {
                    cwd.clone()
                } else {
                    resolve_shell_path(&cwd, args[0])
                };

                match vfs.read_dir(&target) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            println!("(пустой каталог)");
                        }
                        for entry in entries {
                            let entry_path = resolve_shell_path(&target, &entry.name);
                            match vfs.metadata(&entry_path) {
                                Ok(m) => {
                                    let type_tag = match m.kind {
                                        NodeKind::Directory => "[DIR] ",
                                        NodeKind::File => "[FILE]",
                                    };
                                    println!("{type_tag}  0o{:06o}  {:>6} B  {}", m.mode, m.size, entry.name);
                                }
                                Err(_) => {
                                    println!("[????]  ????????  {:>6} B  {}", "?", entry.name);
                                }
                            }
                        }
                    }
                    Err(e) => println!("ls: не удалось прочитать '{target}': {e}"),
                }
            }

            "cat" => {
                if args.is_empty() {
                    println!("Использование: cat <файл>");
                    continue;
                }

                let target = resolve_shell_path(&cwd, args[0]);
                match vfs.open(&target) {
                    Ok(fd) => {
                        let mut buffer = [0u8; 64];
                        let mut content = Vec::new();
                        loop {
                            match vfs.read(fd, &mut buffer) {
                                Ok(0) => break,
                                Ok(n) => content.extend_from_slice(&buffer[..n]),
                                Err(e) => {
                                    println!("cat: ошибка чтения '{target}': {e}");
                                    break;
                                }
                            }
                        }
                        let _ = vfs.close(fd);
                        print!("{}", String::from_utf8_lossy(&content));
                        if !content.ends_with(b"\n") {
                            println!();
                        }
                    }
                    Err(e) => println!("cat: {}: {}", args[0], e),
                }
            }

            "stat" => {
                if args.is_empty() {
                    println!("Использование: stat <путь>");
                    continue;
                }

                let target = resolve_shell_path(&cwd, args[0]);
                match vfs.metadata(&target) {
                    Ok(m) => {
                        println!("  Файл:     {}", target);
                        println!("  Тип:      {:?}", m.kind);
                        println!("  Размер:   {} байт", m.size);
                        println!("  Права:    0o{:06o}", m.mode);
                        println!("  Владелец: UID={}, GID={}", m.uid, m.gid);
                        println!("  Ссылки:   nlink={}", m.nlink);
                        println!("  Время:    mtime={} сек (Unix)", m.mtime);
                    }
                    Err(e) => println!("stat: не удалось получить метаданные '{target}': {e}"),
                }
            }

            "clear" => {
                print!("\x1B[2J\x1B[1;1H");
                io::stdout().flush()?;
            }

            "exit" | "quit" => {
                println!("Завершение работы shell.");
                break;
            }

            other => {
                println!("Неизвестная команда: '{other}'. Введите 'help' для справки.");
            }
        }
    }

    Ok(())
}