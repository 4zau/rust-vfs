# rust-vfs

Прототип VFS с регистрацией файловых систем по префиксам.
Для демонстрации можно подключить CPIO-архив:

```sh
cargo run -- sample.cpio
cargo run -- sample.cpio /initrd/docs
cargo run -- sample.cpio /initrd/docs/readme.txt
```
