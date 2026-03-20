use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use rosc::OscPacket;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_file_path(extension: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("fin-test-{unique}-{counter}.{extension}"))
}

#[test]
fn run_prints_expected_schedule() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 128\n[bd] /4\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
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
fn sounds_lists_local_names_from_overrides() {
    let root = temp_file_path("dir");
    fs::create_dir(&root).expect("root dir should exist");
    let samples_root = root.join("samples");
    fs::create_dir(&samples_root).expect("samples root should exist");
    fs::create_dir(samples_root.join("bd")).expect("bd sample should exist");
    fs::create_dir(samples_root.join("808sd")).expect("808sd sample should exist");

    let superdirt_root = root.join("SuperDirt");
    let synths_dir = superdirt_root.join("synths");
    fs::create_dir_all(&synths_dir).expect("synths dir should exist");
    fs::write(
        synths_dir.join("default-synths.scd"),
        "SynthDef(\\superhat,{})\n~dirt.soundLibrary.addSynth(\\from, (play: {}));\n",
    )
    .expect("synth source should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("sounds")
        .env("FIN_DIRT_SAMPLES_ROOT", &samples_root)
        .env("FIN_SUPERDIRT_ROOT", &superdirt_root)
        .output()
        .expect("command should run");

    fs::remove_dir_all(&root).expect("should clean up temp tree");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sample 808sd"));
    assert!(stdout.contains("sample bd"));
    assert!(stdout.contains("synth superhat"));
    assert!(stdout.contains("synth from"));
}

#[test]
fn run_prints_density_and_shifted_schedule() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[hh] *4 >> 0.25\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "bpm=120\nhh  beat=0.000  bar=0.000\nhh  beat=1.000  bar=0.250\nhh  beat=2.000  bar=0.500\nhh  beat=3.000  bar=0.750\n"
    );
}

#[test]
fn run_accepts_effect_chaining_syntax() {
    let path = temp_file_path("metl");
    fs::write(
        &path,
        "bpm = 120\n[hh] *4 .gain 0.5 .pan -0.25 .speed 1.5 .sustain 0.2\n",
    )
    .expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hh  beat=0.000  bar=0.000"));
    assert!(stdout.contains("hh  beat=3.000  bar=0.750"));
}

#[test]
fn run_prints_sample_index_pattern_body() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[bd] <0 3 5 7> /1\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "bpm=120\nbd:0  beat=0.000  bar=0.000\n"
    );
}

#[test]
fn run_prints_group_pattern_body() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[drum] [bd sd:2] /1\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "bpm=120\nbd  beat=0.000  bar=0.000\nsd:2  beat=0.000  bar=0.000\n"
    );
}

#[test]
fn watch_reloads_on_bar_boundary() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 1200\n[bd] /1\n").expect("should write test file");

    let mut child = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("watch")
        .arg("--no-play")
        .arg("--bars")
        .arg("2")
        .arg(&path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let stdout = child.stdout.take().expect("child should expose stdout");
    let mut reader = BufReader::new(stdout);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .expect("should read watch header");
    assert!(first_line.contains("watch load"));

    fs::write(&path, "bpm = 1200\n[sd] /1\n").expect("should rewrite test file");

    let mut remainder = String::new();
    reader
        .read_to_string(&mut remainder)
        .expect("should read remaining output");
    let status = child.wait().expect("child should exit");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(status.success());
    let stdout = format!("{first_line}{remainder}");
    assert!(stdout.contains("watch load"));
    assert!(stdout.contains("watch reload"));
    assert!(stdout.contains("bd  beat=0.000  bar=0.000"));
    assert!(stdout.contains("sd  beat=0.000  bar=0.000"));
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

#[test]
fn run_sends_osc_packets() {
    let listener = UdpSocket::bind("127.0.0.1:0").expect("listener should bind");
    listener
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("timeout should be configured");
    let port = listener
        .local_addr()
        .expect("listener should have a local address")
        .port();

    let receiver = thread::spawn(move || {
        let mut trigger_count = 0usize;
        let mut buffer = [0u8; 1024];
        while trigger_count < 2 {
            let (size, _) = listener
                .recv_from(&mut buffer)
                .expect("should receive OSC data");
            let packet = rosc::decoder::decode_udp(&buffer[..size])
                .expect("packet should decode")
                .1;
            let OscPacket::Message(message) = packet else {
                continue;
            };

            if message.addr == "/dirt/play" {
                trigger_count += 1;
            }
        }
        trigger_count
    });

    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 960\n[bd] /2\n").expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(receiver.join().expect("receiver should finish"), 2);
}
