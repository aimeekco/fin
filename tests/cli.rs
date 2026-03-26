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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    fs::write(&path, "bpm = 128\n[bd]\n  [bar1] /4\n").expect("should write test file");

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
    fs::write(samples_root.join("bd").join("0.wav"), "").expect("bd audio should exist");
    fs::create_dir(samples_root.join("808sd")).expect("808sd sample should exist");
    fs::write(samples_root.join("808sd").join("1.aif"), "").expect("audio should exist");
    fs::create_dir(samples_root.join("broken")).expect("broken sample should exist");
    fs::write(samples_root.join("broken").join("notes.txt"), "not audio")
        .expect("non-audio file should exist");

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
        .arg("--plain")
        .env("FIN_DIRT_SAMPLES_ROOT", &samples_root)
        .env("FIN_SUPERDIRT_ROOT", &superdirt_root)
        .output()
        .expect("command should run");

    fs::remove_dir_all(&root).expect("should clean up temp tree");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sample 808sd  808-style snare drum"));
    assert!(stdout.contains("sample bd  bass drum / kick"));
    assert!(!stdout.contains("sample broken"));
    assert!(stdout.contains("synth superhat  SynthDef-backed synth `superhat`"));
    assert!(stdout.contains("synth from  registered SuperDirt synth `from`"));
}

#[test]
fn run_prints_density_and_shifted_schedule() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[hh]\n  [bar1] *4 >> 0.25\n").expect("should write test file");

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
        "bpm = 120\n[hh] .gain 0.5 .pan -0.25 .speed 1.5 .sustain 0.2\n  [bar1] *4\n",
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
    fs::write(&path, "bpm = 120\n[bd]\n  [bar1] /1 <0>\n").expect("should write test file");

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
fn run_infers_density_for_atom_sequence_body() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[bd]\n  [bar1] <0 3 5 7>\n").expect("should write test file");

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
        "bpm=120\nbd:0  beat=0.000  bar=0.000\nbd:3  beat=1.000  bar=0.250\nbd:5  beat=2.000  bar=0.500\nbd:7  beat=3.000  bar=0.750\n"
    );
}

#[test]
fn run_prints_group_pattern_body() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[drum]\n  [bar1] /1 [bd sd:2]\n")
        .expect("should write test file");

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
fn run_accepts_default_bar_definition() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[bd]\n  [default] /4\n").expect("should write test file");

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
        "bpm=120\nbd  beat=0.000  bar=0.000\nbd  beat=1.000  bar=0.250\nbd  beat=2.000  bar=0.500\nbd  beat=3.000  bar=0.750\n"
    );
}

#[test]
fn run_plays_intro_before_initial_bar() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\nbars = 4\n[bd]\n  [intro] /1 <8>\n  [bar1] /1 <1>\n")
        .expect("should write test file");

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
        "bpm=120\nbars=4\nbd:8  beat=0.000  bar=0.000\nbpm=120\nbars=4\nbd:1  beat=0.000  bar=0.000\n"
    );
}

#[test]
fn run_accepts_periodic_bar_definition() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\nbars = 4\n[bd]\n  [bar%2] /1 <0>\n")
        .expect("should write test file");

    let output = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("run")
        .arg("--no-play")
        .arg(&path)
        .output()
        .expect("command should run");

    fs::remove_file(&path).expect("should clean up temp file");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "bpm=120\nbars=4\n");
}

#[test]
fn run_prints_note_sequence_body() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 120\n[bass]\n  [bar1] <g4 a4 a3 c3>\n")
        .expect("should write test file");

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
        "bpm=120\nbass@g4  beat=0.000  bar=0.000\nbass@a4  beat=1.000  bar=0.250\nbass@a3  beat=2.000  bar=0.500\nbass@c3  beat=3.000  bar=0.750\n"
    );
}

#[test]
fn watch_reloads_on_bar_boundary() {
    let path = temp_file_path("metl");
    fs::write(&path, "bpm = 1200\nbars = 1\n[bd]\n  [bar1] /1\n").expect("should write test file");

    let mut child = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("watch")
        .arg("--no-play")
        .arg("--bars")
        .arg("3")
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

    fs::write(&path, "bpm = 1200\nbars = 1\n[sd]\n  [bar1] /1\n").expect("should rewrite test file");
    thread::sleep(Duration::from_millis(150));

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
fn superdirt_background_lifecycle_commands_work() {
    let root = temp_file_path("dir");
    fs::create_dir(&root).expect("root dir should exist");
    let state_dir = root.join("state");
    fs::create_dir(&state_dir).expect("state dir should exist");
    let fake_sclang = root.join("fake-sclang.sh");
    fs::write(
        &fake_sclang,
        "#!/usr/bin/env bash\nset -euo pipefail\necho \"fake-sclang $@\"\ntrap 'exit 0' TERM INT\nwhile true; do sleep 1; done\n",
    )
    .expect("fake sclang should be written");
    #[cfg(unix)]
    fs::set_permissions(&fake_sclang, fs::Permissions::from_mode(0o755))
        .expect("fake sclang should be executable");

    let start = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("superdirt")
        .arg("--sclang")
        .arg(&fake_sclang)
        .arg("--port")
        .arg("57129")
        .env("FIN_SUPERDIRT_STATE_DIR", &state_dir)
        .output()
        .expect("command should run");

    assert!(start.status.success());
    let start_stdout = String::from_utf8_lossy(&start.stdout);
    assert!(start_stdout.contains("SuperDirt started in background"));
    assert!(start_stdout.contains("57129"));

    let status = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("superdirt")
        .arg("status")
        .env("FIN_SUPERDIRT_STATE_DIR", &state_dir)
        .output()
        .expect("status should run");

    assert!(status.status.success());
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("SuperDirt running"));
    assert!(status_stdout.contains("57129"));

    let log_path = state_dir.join("superdirt.log");
    let log_contents = wait_for_log_contents(&log_path);
    assert!(log_contents.contains("fake-sclang"));
    assert!(log_contents.contains("57129"));

    let kill = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("superdirt")
        .arg("kill")
        .env("FIN_SUPERDIRT_STATE_DIR", &state_dir)
        .output()
        .expect("kill should run");

    assert!(kill.status.success());
    assert!(String::from_utf8_lossy(&kill.stdout).contains("SuperDirt stopped"));

    let stopped = Command::new(env!("CARGO_BIN_EXE_fin"))
        .arg("superdirt")
        .arg("status")
        .env("FIN_SUPERDIRT_STATE_DIR", &state_dir)
        .output()
        .expect("status should run");

    fs::remove_dir_all(&root).expect("should clean up temp tree");

    assert!(stopped.status.success());
    assert!(String::from_utf8_lossy(&stopped.stdout).contains("SuperDirt is not running"));
}

fn wait_for_log_contents(path: &PathBuf) -> String {
    for _ in 0..20 {
        if let Ok(contents) = fs::read_to_string(path) {
            if contents.contains("fake-sclang") {
                return contents;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }

    fs::read_to_string(path).expect("log should exist")
}

#[test]
fn run_sends_osc_packets() {
    let listener = match UdpSocket::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("listener should bind: {error}"),
    };
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
    fs::write(&path, "bpm = 960\n[bd]\n  [bar1] /2\n").expect("should write test file");

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
