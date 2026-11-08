use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

use cpiofs::CpioFs;
use memfs::MemFs;
use vfs_core::{NodeKind, Vfs};

fn cat_stream(vfs: &mut Vfs, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let fd = vfs.open(path)?;
    let mut buffer = [0u8; 8192];
    let mut stdout = io::stdout().lock();

    let read_res = (|| -> io::Result<()> {
        loop {
            let n = vfs
                .read(fd, &mut buffer)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            if n == 0 {
                break;
            }
            stdout.write_all(&buffer[..n])?;
        }
        stdout.flush()?;
        Ok(())
    })();

    // Всегда закрываем дескриптор
    let _ = vfs.close(fd);
    read_res?;
    Ok(())
}

/// Соединяет текущий cwd шелла и переданный пользователем аргумент.
/// Не нормализует и не убирает суффиксы — передает строку в VFS as-is.
fn to_vfs_path(cwd: &str, input: &str) -> String {
    if input.starts_with('/') {
        input.to_string()
    } else if cwd == "/" {
        format!("/{}", input)
    } else {
        format!("{}/{}", cwd, input)
    }
}

/// Создает VFS, монтирует корневую MemFs в `/` и CpioFs в `/initrd`
fn setup_vfs(archive_bytes: Vec<u8>) -> Result<Vfs, Box<dyn std::error::Error>> {
    let mut vfs = Vfs::new();

    // 1. Корневая MemFs
    let root_fs = MemFs::default();
    vfs.register("/", Box::new(root_fs))?;

    // 2. CpioFs с архивом под /initrd
    // Если конструктор принимает срез (&archive_bytes), передайте срез
    let cpio_fs = CpioFs::from_bytes(archive_bytes)?;
    vfs.register("/initrd", Box::new(cpio_fs))?;

    Ok(vfs)
}

/// Интерактивный режим Shell
fn run_shell(mut vfs: Vfs) -> Result<(), Box<dyn std::error::Error>> {
    println!("=====================================================");
    println!("  Интерактивный shell Rust VFS                       ");
    println!("  Архив смонтирован в '/initrd'                      ");
    println!("  Введите 'help' для справки или 'mounts' для точек  ");
    println!("=====================================================\n");

    let mut cwd = String::from("/");
    let stdin = io::stdin();

    loop {
        print!("vfs:{cwd}> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break; // EOF (Ctrl+D)
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Разделяем только по первому пробелу, сохраняя пробелы в путях файлов!
        let (cmd, arg) = match line.split_once(char::is_whitespace) {
            Some((c, a)) => (c.trim(), a.trim()),
            None => (line, ""),
        };

        match cmd {
            "help" => {
                println!("Команды шелла:");
                println!("  mounts         - показать смонтированные точки VFS");
                println!("  ls [путь]      - список файлов в каталоге");
                println!("  cd <путь>      - сменить текущий каталог");
                println!("  pwd            - показать текущий путь");
                println!("  cat <файл>     - вывести содержимое файла");
                println!("  stat <путь>    - показать метаданные файла/папки");
                println!("  clear          - очистить экран");
                println!("  exit / quit    - выйти из shell");
            }

            "mounts" => {
                println!("Смонтированные префиксы:");
                for (id, prefix) in vfs.registered_prefixes() {
                    println!("  {prefix:<12} (ID: {id:?})");
                }
            }

            "pwd" => {
                println!("{cwd}");
            }

            "cd" => {
                if arg.is_empty() || arg == "/" {
                    cwd = "/".to_string();
                } else if arg == ".." {
                    if cwd != "/" {
                        if let Some(pos) = cwd.rfind('/') {
                            if pos == 0 {
                                cwd = "/".to_string();
                            } else {
                                cwd.truncate(pos);
                            }
                        }
                    }
                } else if arg == "." {
                    // остаемся на месте
                } else {
                    let target = to_vfs_path(&cwd, arg);
                    match vfs.metadata(&target) {
                        Ok(meta) => {
                            if meta.kind == NodeKind::Directory {
                                let clean = target.trim_end_matches('/');
                                cwd = if clean.is_empty() {
                                    "/".to_string()
                                } else {
                                    clean.to_string()
                                };
                            } else {
                                eprintln!("cd: not a directory: {arg}");
                            }
                        }
                        Err(e) => eprintln!("cd: {arg}: {e}"),
                    }
                }
            }

            "ls" => {
                let target = if arg.is_empty() {
                    cwd.clone()
                } else {
                    to_vfs_path(&cwd, arg)
                };

                match vfs.read_dir(&target) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            println!("(пустой каталог)");
                        }
                        for entry in entries {
                            let entry_path = if target == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", target.trim_end_matches('/'), entry.name)
                            };

                            match vfs.metadata(&entry_path) {
                                Ok(m) => {
                                    let type_tag = match m.kind {
                                        NodeKind::Directory => "[DIR] ",
                                        NodeKind::File => "[FILE]",
                                    };
                                    println!("{type_tag}  0o{:06o}  {:>7} B  {}", m.mode, m.size, entry.name);
                                }
                                Err(_) => {
                                    println!("[????]  ????????  {:>7} B  {}", "?", entry.name);
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("ls: cannot access '{target}': {e}"),
                }
            }

            "cat" => {
                if arg.is_empty() {
                    eprintln!("Использование: cat <файл>");
                    continue;
                }

                let target = to_vfs_path(&cwd, arg);
                if let Err(e) = cat_stream(&mut vfs, &target) {
                    eprintln!("cat: {arg}: {e}");
                }
            }

            "stat" => {
                if arg.is_empty() {
                    eprintln!("Использование: stat <путь>");
                    continue;
                }

                let target = to_vfs_path(&cwd, arg);
                match vfs.metadata(&target) {
                    Ok(m) => {
                        println!("  Файл:     {target}");
                        println!("  Тип:      {:?}", m.kind);
                        println!("  Размер:   {} байт", m.size);
                        println!("  Права:    0o{:06o}", m.mode);
                        println!("  Владелец: UID={}, GID={}", m.uid, m.gid);
                        println!("  Ссылки:   nlink={}", m.nlink);
                        println!("  Время:    mtime={} сек", m.mtime);
                    }
                    Err(e) => eprintln!("stat: {arg}: {e}"),
                }
            }

            "clear" => {
                print!("\x1B[2J\x1B[1;1H");
                io::stdout().flush()?;
            }

            "exit" | "quit" => {
                break;
            }

            other => {
                eprintln!("Неизвестная команда: '{other}'. Введите 'help'.");
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    match args.as_slice() {
        // Сценарий 1: Старый CLI (cargo run -- sample.cpio /initrd/docs/readme.txt)
        [_, archive_path, vfs_path] if archive_path != "shell" => {
            let bytes = fs::read(archive_path)?;
            let mut vfs = setup_vfs(bytes)?;
            cat_stream(&mut vfs, vfs_path)?;
        }

        // Сценарий 2: Режим shell с архивом (cargo run -- shell sample.cpio)
        [_, mode, archive_path] if mode == "shell" => {
            let bytes = fs::read(archive_path)?;
            let vfs = setup_vfs(bytes)?;
            run_shell(vfs)?;
        }

        // Сценарий 3: Запуск shell без архива (чистая MemFs)
        [_, mode] if mode == "shell" => {
            let mut vfs = Vfs::new();
            vfs.register("/", Box::new(MemFs::default()))?;
            run_shell(vfs)?;
        }

        _ => {
            eprintln!("Использование:");
            eprintln!("  1) Чтение файла из архива (CI / старый CLI):");
            eprintln!("     cargo run -- <archive.cpio> <vfs_path>");
            eprintln!("     Пример: cargo run -- crates/cpio-read/tests/fixtures/sample.cpio /initrd/docs/readme.txt\n");
            eprintln!("  2) Интерактивный shell:");
            eprintln!("     cargo run -- shell <archive.cpio>");
            eprintln!("     Пример: cargo run -- shell crates/cpio-read/tests/fixtures/sample.cpio");
            process::exit(1);
        }
    }

    Ok(())
}