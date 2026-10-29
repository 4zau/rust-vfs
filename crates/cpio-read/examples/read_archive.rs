use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: cargo run -p cpio-read --example read_archive -- <archive.cpio>")?;
    
    let bytes = fs::read(path)?;
    for entry in cpio_read::read_archive(&bytes)? {
        println!(
            "{}\t{} bytes\tmode {:o}",
            entry.name,
            entry.data.len(),
            entry.mode
        );
    }
    Ok(())
}
