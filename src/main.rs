use std::{vec};

use vfs_core::*;
use memfs::MemFs;

fn main() {
    let mut memfs = MemFs::new();
    
    let file_node = memfs.create(memfs.root(), "hello.txt", NodeKind::File).unwrap();
    let mut res = memfs.write(file_node, 0, b"hello world!").unwrap();

    println!("{res}");

    let root_dir = memfs.read_dir(memfs.root()).unwrap();

    for entry in root_dir {
        println!("{name}, {id:?}", name = entry.name, id = entry.node);
    }

    let mut content: Vec<u8> = vec![0; 15];
    res = memfs.read(file_node, 0, &mut content).unwrap();

    println!("{res}");

    for byte in content {
        print!("{char} ", char = byte as char);
    }
}