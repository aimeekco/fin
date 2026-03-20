use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_file_path(extension: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!("fin-test-{unique}.{extension}"))
}

#[test]
fn run_prints_expected_schedule() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 128\n[bd] /4\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "bpm=128\nbd  beat=0.000  bar=0.000\nbd  beat=1.000  bar=0.250\nbd  beat=2.000  bar=0.500\nbd  beat=3.000  bar=0.750\n"
    );
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
}

#[test]
fn run_rejects_wrong_extension() {
    let path = temp_file_path("mtl");
    fs::write(&path, "[bd] /4\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected a `.metl` source file"));
}
