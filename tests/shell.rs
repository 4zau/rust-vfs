use std::io::Write;
use std::process::{Command, Stdio};

const ARCHIVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/sample.cpio");

#[test]
fn shell_lists_archive_at_root_and_normalizes_paths() {
    let mut shell = Command::new(env!("CARGO_BIN_EXE_rust-vfs"))
        .args(["shell", ARCHIVE])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut input = shell.stdin.take().unwrap();
    input
        .write_all(b"ls /\ncd //docs/./\npwd\ncat ../docs/readme.txt\ncd ..\npwd\nexit\n")
        .unwrap();
    drop(input);

    let output = shell.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("[DIR]") && line.ends_with("  docs")),
        "{stdout}"
    );
    assert!(stdout.contains("vfs:/docs> /docs\n"), "{stdout}");
    assert!(stdout.contains("vfs:/> /\n"), "{stdout}");
    assert!(stdout.contains("Nested file\n"), "{stdout}");
}

#[test]
fn standalone_reads_archive_path_from_root() {
    let output = Command::new(env!("CARGO_BIN_EXE_rust-vfs"))
        .args([ARCHIVE, "/docs/readme.txt"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Nested file\n");
}
