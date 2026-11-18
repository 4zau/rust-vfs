# zerofs

`no_std` файловая система с единственным файлом `zero`, реализующая
`FileSystem` из `vfs-core`. Чтение заполняет весь буфер нулями при любом
смещении, запись принимает все байты и отбрасывает их. Пустые операции
возвращают 0. Данные файла не хранятся.

```rust
use vfs_core::Vfs;
use zerofs::ZeroFs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut vfs = Vfs::new();
    vfs.register("/dev", Box::new(ZeroFs::new()))?;
    let fd = vfs.open("/dev/zero")?;
    let mut buffer = [0xff; 1024];
    assert_eq!(vfs.read(fd, &mut buffer)?, buffer.len());
    assert_eq!(buffer, [0; 1024]);
    vfs.close(fd)?;
    Ok(())
}
```

Корень — каталог с одним файлом. Размер `zero` в метаданных равен 0,
но чтение непустого буфера не возвращает EOF. Права файла — 0666.
Поскольку `vfs-core` пока не поддерживает символьные устройства, узел
представлен как `NodeKind::File` с соответствующими битами типа в `mode`.
Изменение структуры, метаданных и `truncate` возвращают `Unsupported`.

Смещения используют текущий тип `usize` из `vfs-core`. Сам `ZeroFs`
игнорирует смещение; чтение через `Vfs` ограничено переполнением его
счётчика позиции (`SizeOverflow`).
