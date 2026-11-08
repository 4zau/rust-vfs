# memfs

Файловая система в оперативной памяти, реализующая `FileSystem` из `vfs-core`.
Поддерживает только  обычные файлы и каталоги.

## Структура и хранение

`MemFs::default()` создают пустую ФС с корневым каталогом
`NodeId::new(0)`. Узлы хранятся в `BTreeMap<NodeId, Node>`:
каждый узел содержит метаданные и данные: файл — `Vec<u8>`,
каталог — `BTreeMap<String, NodeId>` своих детей.

`NodeId` локален для экземпляра MemFs. Новые ID выдаются последовательно
и не используются повторно после удаления. При исчерпании `u64` создание
возвращает `FsError::IdentifierExhausted`.

## Пример использования

```rust
use memfs::MemFs;
use vfs_core::{FileSystem, NodeKind};

fn main() -> vfs_core::Result<()> {
    let mut fs = MemFs::new();
    let root = fs.root();
    let docs = fs.create(root, "docs", NodeKind::Directory)?;
    let file = fs.create(docs, "hello.txt", NodeKind::File)?;
    fs.write(file, 0, b"hello")?;

    assert_eq!(fs.lookup(docs, "hello.txt")?, file);
    assert_eq!(fs.metadata(file)?.size, 5);
    assert_eq!(fs.read_dir(docs)?[0].name, "hello.txt");

    let mut buffer = [0; 3];
    let first = fs.read(file, 0, &mut buffer)?;
    assert_eq!(&buffer[..first], b"hel");
    let second = fs.read(file, first, &mut buffer)?;
    assert_eq!(&buffer[..second], b"lo");

    fs.remove(docs, "hello.txt")?;
    fs.remove(root, "docs")?;
    Ok(())
}
```

## Операции

| Метод | Поведение |
| --- | --- |
| `root()` | ID корневого каталога |
| `lookup(parent, name)` | Найти ребёнка каталога по имени |
| `metadata(node)` | Тип, текущий размер и сохранённые атрибуты CPIO newc |
| `set_metadata(node, update)` | Изменить выбранные атрибуты существующего узла |
| `read_dir(directory)` | Непосредственные дети в порядке имён, без `.` и `..` |
| `create(parent, name, kind)` | Создать пустой файл или каталог |
| `read(file, offset, buffer)` | Скопировать доступные байты начиная с offset |
| `write(file, offset, data)` | Записать байты, при необходимости расширить файл |
| `truncate(file, length)` | Укоротить или расширить файл |
| `remove(parent, name)` | Удалить файл или пустой каталог |

Пустое имя, `.`, `..` и имя с `/` отвергаются с `InvalidName`.
Родитель должен существовать и быть каталогом; имя должно быть свободным.

### Чтение и изменение размера

`read` возвращает число скопированных байтов, не превышающее размер буфера.
На EOF, за EOF и при пустом буфере возвращается `0` для существующего файла.
Хвост буфера после скопированных байтов остаётся неизменным.

`write` перезаписывает диапазон; при записи за концом файла промежуток
заполняется нулями. Пустая запись не расширяет файл даже при большом смещении.
`truncate` удаляет хвост при уменьшении и добавляет нули при увеличении.

### Удаление

Непустой каталог удалить нельзя (`DirectoryNotEmpty`). После удаления узел
больше не существует, его старый `NodeId` даёт `NotFound` и не переиспользуется.

## Подключение к VFS

```rust
use memfs::MemFs;
use vfs_core::{FileSystem, NodeKind, Vfs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut fs = MemFs::new();
    let file = fs.create(fs.root(), "config.txt", NodeKind::File)?;
    fs.write(file, 0, b"mode=demo\n")?;

    let mut vfs = Vfs::new();
    let id = vfs.register("/data", Box::new(fs))?;

    assert_eq!(vfs.metadata("/data/config.txt")?.size, 10);

    let fd = vfs.open("/data/config.txt")?;
    let mut buffer = [0; 32];
    let count = vfs.read(fd, &mut buffer)?;
    assert_eq!(&buffer[..count], b"mode=demo\n");

    vfs.close(fd)?;
    vfs.unregister(id)?;
    Ok(())
}
```

## Метаданные CPIO newc

`Metadata` содержит `kind`, `size`, `mode` (тип и права Unix),
`uid`, `gid`, `nlink`, `mtime` (секунды Unix).
Размер файла вычисляется по содержимому, каталога — 0.

`Metadata::new(kind)` задаёт права 0644 для файла и 0755 для каталога,
UID/GID 0, `nlink = 1`; остальные числовые атрибуты равны 0.
Эти значения используются при обычном `create` и для корня.

```rust
use vfs_core::MetadataUpdate;

fs.set_metadata(file, MetadataUpdate {
    permissions: Some(0o640),
    uid: Some(1000),
    ..MetadataUpdate::default()
})?;
```
