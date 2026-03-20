use std::env;
use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

const SCLANG_ENV_VAR: &str = "FIN_SCLANG_BIN";
const STATE_DIR_ENV_VAR: &str = "FIN_SUPERDIRT_STATE_DIR";
const MACOS_APP_SCLANG: &str = "/Applications/SuperCollider.app/Contents/MacOS/sclang";
const STARTUP_SCRIPT: &str = include_str!("../supercollider/superdirt_startup.scd");
const STATE_FILE_NAME: &str = "superdirt.state";
const LOG_FILE_NAME: &str = "superdirt.log";
const SCRIPT_FILE_NAME: &str = "superdirt_startup.scd";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartMode {
    Background,
    Foreground,
}

pub fn start_superdirt(
    sclang_override: Option<PathBuf>,
    port: u16,
    mode: StartMode,
) -> Result<(), String> {
    let sclang_path = resolve_sclang_path(sclang_override.as_deref())?;
    let state_dir = state_dir()?;
    fs::create_dir_all(&state_dir)
        .map_err(|error| format!("failed to create {}: {error}", state_dir.display()))?;
    cleanup_stale_state(&state_dir)?;
    let script_path = write_startup_script(&state_dir)?;

    match mode {
        StartMode::Foreground => run_superdirt_foreground(&sclang_path, &script_path, port),
        StartMode::Background => {
            run_superdirt_background(&state_dir, &sclang_path, &script_path, port)
        }
    }
}

pub fn stop_superdirt() -> Result<(), String> {
    let state_dir = state_dir()?;
    let Some(state) = load_state(&state_dir)? else {
        println!("SuperDirt is not running");
        return Ok(());
    };

    if !process_running(state.pid) {
        cleanup_state_files(&state)?;
        println!("SuperDirt was not running; cleaned up stale state");
        return Ok(());
    }

    terminate_process(state.pid)?;
    wait_for_exit(state.pid, 3_000);
    if process_running(state.pid) {
        force_kill_process(state.pid)?;
        wait_for_exit(state.pid, 1_000);
    }

    cleanup_state_files(&state)?;
    println!("SuperDirt stopped (pid={})", state.pid);
    Ok(())
}

pub fn superdirt_status() -> Result<(), String> {
    let state_dir = state_dir()?;
    let Some(state) = load_state(&state_dir)? else {
        println!("SuperDirt is not running");
        return Ok(());
    };

    if process_running(state.pid) {
        println!(
            "SuperDirt running pid={} port={} log={}",
            state.pid,
            state.port,
            state.log_path.display()
        );
    } else {
        cleanup_state_files(&state)?;
        println!("SuperDirt is not running");
    }

    Ok(())
}

fn run_superdirt_foreground(
    sclang_path: &Path,
    script_path: &Path,
    port: u16,
) -> Result<(), String> {
    let status = Command::new(&sclang_path)
        .arg(script_path)
        .arg(port.to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("failed to launch `{}`: {error}", sclang_path.display()))?;

    let _ = fs::remove_file(script_path);

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{}` exited with status {status}",
            sclang_path.display()
        ))
    }
}

fn run_superdirt_background(
    state_dir: &Path,
    sclang_path: &Path,
    script_path: &Path,
    port: u16,
) -> Result<(), String> {
    if let Some(state) = load_state(state_dir)? {
        if process_running(state.pid) {
            return Err(format!(
                "SuperDirt is already running (pid={}, port={}). Use `fin superdirt kill` first.",
                state.pid, state.port
            ));
        }
        cleanup_state_files(&state)?;
    }

    let log_path = state_dir.join(LOG_FILE_NAME);
    let log_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path)
        .map_err(|error| format!("failed to open {}: {error}", log_path.display()))?;
    let stderr_file = log_file
        .try_clone()
        .map_err(|error| format!("failed to clone log handle: {error}"))?;

    let mut command = Command::new(sclang_path);
    command
        .arg(script_path)
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_file));
    configure_background_process(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to launch `{}`: {error}", sclang_path.display()))?;
    let pid = child.id();

    thread::sleep(std::time::Duration::from_millis(250));
    if let Some(status) = child
        .try_wait()
        .map_err(|error| format!("failed to inspect background process: {error}"))?
    {
        let _ = fs::remove_file(script_path);
        return Err(format!(
            "SuperDirt exited immediately with status {status}. Check {}",
            log_path.display()
        ));
    }

    let state = SuperdirtState {
        pid,
        port,
        script_path: script_path.to_path_buf(),
        log_path: log_path.clone(),
        state_path: state_dir.join(STATE_FILE_NAME),
    };
    write_state(&state)?;

    println!(
        "SuperDirt started in background pid={} port={} log={}",
        pid,
        port,
        log_path.display()
    );
    Ok(())
}

fn write_startup_script(state_dir: &Path) -> Result<PathBuf, String> {
    let path = state_dir.join(SCRIPT_FILE_NAME);
    fs::write(&path, STARTUP_SCRIPT)
        .map_err(|error| format!("failed to write startup script: {error}"))?;
    Ok(path)
}

pub fn resolve_sclang_path(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return validate_sclang_path(path);
    }

    if let Some(path) = env::var_os(SCLANG_ENV_VAR) {
        return validate_sclang_path(Path::new(&path));
    }

    if let Some(path) = resolve_from_path() {
        return Ok(path);
    }

    let macos_path = PathBuf::from(MACOS_APP_SCLANG);
    if macos_path.is_file() {
        return Ok(macos_path);
    }

    Err(format!(
        "could not locate `sclang`. Pass `--sclang <path>` or set `{SCLANG_ENV_VAR}`."
    ))
}

fn resolve_from_path() -> Option<PathBuf> {
    let binary_name = sclang_binary_name();
    let path_var = env::var_os("PATH")?;

    env::split_paths(&path_var)
        .map(|entry| entry.join(&binary_name))
        .find(|candidate| candidate.is_file())
}

fn validate_sclang_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "`{}` is not a valid `sclang` binary path",
            path.display()
        ))
    }
}

fn sclang_binary_name() -> OsString {
    if cfg!(windows) {
        OsString::from("sclang.exe")
    } else {
        OsString::from("sclang")
    }
}

fn state_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os(STATE_DIR_ENV_VAR) {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| "could not determine a home directory for SuperDirt state".to_string())?;
    Ok(PathBuf::from(home).join(".fin"))
}

fn cleanup_stale_state(state_dir: &Path) -> Result<(), String> {
    let Some(state) = load_state(state_dir)? else {
        return Ok(());
    };
    if !process_running(state.pid) {
        cleanup_state_files(&state)?;
    }
    Ok(())
}

fn load_state(state_dir: &Path) -> Result<Option<SuperdirtState>, String> {
    let state_path = state_dir.join(STATE_FILE_NAME);
    if !state_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&state_path)
        .map_err(|error| format!("failed to read {}: {error}", state_path.display()))?;
    parse_state(&contents, state_path).map(Some)
}

fn parse_state(contents: &str, state_path: PathBuf) -> Result<SuperdirtState, String> {
    let mut pid = None;
    let mut port = None;
    let mut script_path = None;
    let mut log_path = None;

    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "pid" => {
                pid =
                    Some(value.parse::<u32>().map_err(|error| {
                        format!("invalid pid in {}: {error}", state_path.display())
                    })?)
            }
            "port" => {
                port = Some(value.parse::<u16>().map_err(|error| {
                    format!("invalid port in {}: {error}", state_path.display())
                })?)
            }
            "script_path" => script_path = Some(PathBuf::from(value)),
            "log_path" => log_path = Some(PathBuf::from(value)),
            _ => {}
        }
    }

    Ok(SuperdirtState {
        pid: pid.ok_or_else(|| format!("missing pid in {}", state_path.display()))?,
        port: port.ok_or_else(|| format!("missing port in {}", state_path.display()))?,
        script_path: script_path
            .ok_or_else(|| format!("missing script path in {}", state_path.display()))?,
        log_path: log_path
            .ok_or_else(|| format!("missing log path in {}", state_path.display()))?,
        state_path,
    })
}

fn write_state(state: &SuperdirtState) -> Result<(), String> {
    let contents = format!(
        "pid={}\nport={}\nscript_path={}\nlog_path={}\n",
        state.pid,
        state.port,
        state.script_path.display(),
        state.log_path.display()
    );
    fs::write(&state.state_path, contents)
        .map_err(|error| format!("failed to write {}: {error}", state.state_path.display()))
}

fn cleanup_state_files(state: &SuperdirtState) -> Result<(), String> {
    remove_if_exists(&state.state_path)?;
    remove_if_exists(&state.script_path)?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
    }
}

#[cfg(unix)]
fn configure_background_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_background_process(_: &mut Command) {}

#[cfg(unix)]
fn process_running(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        return true;
    }

    matches!(io::Error::last_os_error().raw_os_error(), Some(code) if code == libc::EPERM)
}

#[cfg(not(unix))]
fn process_running(_: u32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> Result<(), String> {
    send_signal(pid, libc::SIGTERM)
}

#[cfg(not(unix))]
fn terminate_process(_: u32) -> Result<(), String> {
    Err("process control is not implemented on this platform yet".to_string())
}

#[cfg(unix)]
fn force_kill_process(pid: u32) -> Result<(), String> {
    send_signal(pid, libc::SIGKILL)
}

#[cfg(not(unix))]
fn force_kill_process(_: u32) -> Result<(), String> {
    Err("process control is not implemented on this platform yet".to_string())
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: i32) -> Result<(), String> {
    let result = unsafe { libc::kill(pid as i32, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!(
            "failed to signal process {}: {}",
            pid,
            io::Error::last_os_error()
        ))
    }
}

fn wait_for_exit(pid: u32, timeout_ms: u64) {
    let mut waited = 0u64;
    while process_running(pid) && waited < timeout_ms {
        thread::sleep(std::time::Duration::from_millis(50));
        waited += 50;
    }
}

#[derive(Debug)]
struct SuperdirtState {
    pid: u32,
    port: u16,
    script_path: PathBuf,
    log_path: PathBuf,
    state_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::{parse_state, resolve_sclang_path, sclang_binary_name};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_file_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("fin-supercollider-test-{unique}-{counter}"))
    }

    #[test]
    fn explicit_sclang_path_is_accepted() {
        let path = temp_file_path();
        fs::write(&path, "").expect("test file should be created");

        let resolved = resolve_sclang_path(Some(&path)).expect("path should resolve");

        fs::remove_file(&path).expect("test file should be removed");

        assert_eq!(resolved, path);
    }

    #[test]
    fn missing_explicit_sclang_path_is_rejected() {
        let error =
            resolve_sclang_path(Some(PathBuf::from("/definitely/missing-sclang").as_path()))
                .expect_err("missing path should fail");

        assert!(error.contains("is not a valid `sclang` binary path"));
    }

    #[test]
    fn binary_name_matches_platform() {
        let expected = if cfg!(windows) {
            "sclang.exe"
        } else {
            "sclang"
        };
        assert_eq!(sclang_binary_name().to_string_lossy(), expected);
    }

    #[test]
    fn parses_state_file() {
        let state_path = PathBuf::from("/tmp/superdirt.state");
        let state = parse_state(
            "pid=42\nport=57120\nscript_path=/tmp/start.scd\nlog_path=/tmp/superdirt.log\n",
            state_path.clone(),
        )
        .expect("state should parse");

        assert_eq!(state.pid, 42);
        assert_eq!(state.port, 57120);
        assert_eq!(state.script_path, PathBuf::from("/tmp/start.scd"));
        assert_eq!(state.log_path, PathBuf::from("/tmp/superdirt.log"));
        assert_eq!(state.state_path, state_path);
    }
}
