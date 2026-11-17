use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn shell_lists_archive_mount_and_normalizes_paths() {
    let archive = concat!(env!("CARGO_MANIFEST_DIR"), "/sample.cpio");
    let mut shell = Command::new(env!("CARGO_BIN_EXE_rust-vfs"))
        .args(["shell", archive])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut input = shell.stdin.take().unwrap();
    input
        .write_all(b"ls /\ncd //initrd/./\npwd\ncd ..\npwd\ncd /initrd/docs\ncat ../docs/readme.txt\nexit\n")
        .unwrap();
    drop(input);

    let output = shell.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("[DIR]") && line.ends_with("  initrd")),
        "{stdout}"
    );
    assert!(stdout.contains("vfs:/initrd> /initrd\n"), "{stdout}");
    assert!(stdout.contains("vfs:/> /\n"), "{stdout}");
    assert!(stdout.contains("Nested file\n"), "{stdout}");
}
