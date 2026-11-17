use std::collections::BTreeSet;
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
                .map_err(|e| io::Error::other(e.to_string()))?;
            if n == 0 {
                break;
            }
            stdout.write_all(&buffer[..n])?;
        }
        stdout.flush()?;
        Ok(())
    })();

    let _ = vfs.close(fd);
    read_res?;
    Ok(())
}

fn to_vfs_path(cwd: &str, input: &str) -> String {
    let mut path = if input.starts_with('/') || cwd == "/" {
        String::new()
    } else {
        cwd.to_owned()
    };

    for component in input.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if let Some(separator) = path.rfind('/') {
                    path.truncate(separator);
                }
            }
            name => {
                path.push('/');
                path.push_str(name);
            }
        }
    }

    // Keep directory-only paths such as `file/.` invalid for regular files.
    if path.is_empty() || input.ends_with('/') || input.rsplit('/').next() == Some(".") {
        path.push('/');
    }
    path
}

fn setup_vfs(archive_bytes: Vec<u8>) -> Result<Vfs, Box<dyn std::error::Error>> {
    let mut vfs = Vfs::new();

    let root_fs = MemFs::default();
    vfs.register("/", Box::new(root_fs))?;

    let cpio_fs = CpioFs::from_bytes(archive_bytes)?;
    vfs.register("/initrd", Box::new(cpio_fs))?;

    Ok(vfs)
}

fn run_shell(mut vfs: Vfs, archive_mounted: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("=====================================================");
    println!("  Интерактивный shell Rust VFS                       ");
    if archive_mounted {
        println!("  Архив смонтирован в '/initrd'                      ");
    }
    println!("  Введите 'help' для справки или 'mounts' для точек  ");
    println!("=====================================================\n");

    let mut cwd = String::from("/");
    let stdin = io::stdin();

    loop {
        print!("vfs:{cwd}> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (cmd, arg) = match line.split_once(char::is_whitespace) {
            Some((c, a)) => (c.trim(), a.trim()),
            None => (line, ""),
        };
        if matches!(cmd, "cd" | "ls" | "cat" | "stat") && arg.contains('\0') {
            eprintln!("{cmd}: invalid path");
            continue;
        }

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
                let mut target = to_vfs_path(&cwd, if arg.is_empty() { "/" } else { arg });
                match vfs.metadata(&target) {
                    Ok(meta) if meta.kind == NodeKind::Directory => {
                        if target.len() > 1 && target.ends_with('/') {
                            target.pop();
                        }
                        cwd = target;
                    }
                    Ok(_) => eprintln!("cd: not a directory: {arg}"),
                    Err(e) => eprintln!("cd: {arg}: {e}"),
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
                        let mut names: BTreeSet<&str> =
                            entries.iter().map(|entry| entry.name.as_str()).collect();
                        if target == "/" {
                            names.extend(vfs.registered_prefixes().filter_map(|(_, prefix)| {
                                let name = prefix.strip_prefix('/')?;
                                (!name.is_empty() && !name.contains('/')).then_some(name)
                            }));
                        }
                        if names.is_empty() {
                            println!("(пустой каталог)");
                        }
                        for name in names {
                            let entry_path = format!("{}/{name}", target.trim_end_matches('/'));

                            match vfs.metadata(&entry_path) {
                                Ok(m) => {
                                    let type_tag = match m.kind {
                                        NodeKind::Directory => "[DIR] ",
                                        NodeKind::File => "[FILE]",
                                    };
                                    println!(
                                        "{type_tag}  0o{:06o}  {:>7} B  {name}",
                                        m.mode, m.size
                                    );
                                }
                                Err(_) => {
                                    println!("[????]  ????????  {:>7} B  {name}", "?");
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
        [_, archive_path, vfs_path] if archive_path != "shell" => {
            let bytes = fs::read(archive_path)?;
            let mut vfs = setup_vfs(bytes)?;
            cat_stream(&mut vfs, vfs_path)?;
        }

        [_, mode, archive_path] if mode == "shell" => {
            let bytes = fs::read(archive_path)?;
            let vfs = setup_vfs(bytes)?;
            run_shell(vfs, true)?;
        }

        [_, mode] if mode == "shell" => {
            let mut vfs = Vfs::new();
            vfs.register("/", Box::new(MemFs::default()))?;
            run_shell(vfs, false)?;
        }

        _ => {
            eprintln!("Использование:");
            eprintln!("  1) Чтение файла из архива:");
            eprintln!("     cargo run -- <archive.cpio> <vfs_path>");
            eprintln!("     Пример: cargo run -- sample.cpio /initrd/docs/readme.txt\n");
            eprintln!("  2) Интерактивный shell:");
            eprintln!("     cargo run -- shell <archive.cpio>");
            eprintln!("     Пример: cargo run -- shell sample.cpio");
            eprintln!("     Без архива: cargo run -- shell");
            process::exit(1);
        }
    }

    Ok(())
}
