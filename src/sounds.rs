use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const FIN_DIRT_SAMPLES_ROOT: &str = "FIN_DIRT_SAMPLES_ROOT";
const FIN_SUPERDIRT_ROOT: &str = "FIN_SUPERDIRT_ROOT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundsReport {
    pub samples_root: Option<PathBuf>,
    pub sample_names: Vec<String>,
    pub superdirt_root: Option<PathBuf>,
    pub synth_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundsError {
    message: String,
}

impl SoundsError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl Error for SoundsError {}

pub fn load_sounds_report() -> Result<SoundsReport, SoundsError> {
    let samples_root = resolve_samples_root();
    let superdirt_root = resolve_superdirt_root();

    let sample_names = match &samples_root {
        Some(root) => collect_sample_names(root)?,
        None => Vec::new(),
    };
    let synth_names = match &superdirt_root {
        Some(root) => collect_synth_names(root)?,
        None => Vec::new(),
    };

    if samples_root.is_none() && superdirt_root.is_none() {
        return Err(SoundsError::new(
            "could not locate SuperDirt samples or quark install. Set `FIN_DIRT_SAMPLES_ROOT` and/or `FIN_SUPERDIRT_ROOT` if needed.",
        ));
    }

    Ok(SoundsReport {
        samples_root,
        sample_names,
        superdirt_root,
        synth_names,
    })
}

pub fn format_sounds_report(report: &SoundsReport) -> String {
    let mut lines = Vec::new();

    if let Some(root) = &report.samples_root {
        lines.push(format!("samples_root={}", root.display()));
        lines.push(format!("sample_count={}", report.sample_names.len()));
        lines.extend(
            report
                .sample_names
                .iter()
                .map(|name| format!("sample {name}")),
        );
    } else {
        lines.push("samples_root=not_found".to_string());
    }

    if let Some(root) = &report.superdirt_root {
        lines.push(format!("superdirt_root={}", root.display()));
        lines.push(format!("synth_count={}", report.synth_names.len()));
        lines.extend(
            report
                .synth_names
                .iter()
                .map(|name| format!("synth {name}")),
        );
    } else {
        lines.push("superdirt_root=not_found".to_string());
    }

    lines.join("\n")
}

fn resolve_samples_root() -> Option<PathBuf> {
    env::var_os(FIN_DIRT_SAMPLES_ROOT)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            default_supercollider_root()
                .join("downloaded-quarks")
                .join("Dirt-Samples")
                .is_dir()
                .then(|| {
                    default_supercollider_root()
                        .join("downloaded-quarks")
                        .join("Dirt-Samples")
                })
        })
}

fn resolve_superdirt_root() -> Option<PathBuf> {
    env::var_os(FIN_SUPERDIRT_ROOT)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            default_supercollider_root()
                .join("downloaded-quarks")
                .join("SuperDirt")
                .is_dir()
                .then(|| {
                    default_supercollider_root()
                        .join("downloaded-quarks")
                        .join("SuperDirt")
                })
        })
}

fn default_supercollider_root() -> PathBuf {
    let home = env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("SuperCollider")
}

fn collect_sample_names(root: &Path) -> Result<Vec<String>, SoundsError> {
    let entries = fs::read_dir(root)
        .map_err(|error| SoundsError::new(format!("failed to read {}: {error}", root.display())))?;
    let mut names = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            SoundsError::new(format!(
                "failed to inspect entries in {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with('.') {
                names.push(name.to_string());
            }
        }
    }

    names.sort();
    Ok(names)
}

fn collect_synth_names(root: &Path) -> Result<Vec<String>, SoundsError> {
    let mut names = BTreeSet::new();
    let synth_def_pattern = Regex::new(r#"SynthDef\(\s*\\([A-Za-z0-9_]+)"#)
        .map_err(|error| SoundsError::new(format!("invalid synth regex: {error}")))?;
    let add_synth_pattern = Regex::new(r#"addSynth\(\s*\\([A-Za-z0-9_]+)"#)
        .map_err(|error| SoundsError::new(format!("invalid addSynth regex: {error}")))?;

    for relative in [
        Path::new("synths"),
        Path::new("library"),
        Path::new("hacks"),
    ] {
        let directory = root.join(relative);
        if directory.is_dir() {
            collect_synth_names_from_dir(
                &directory,
                &synth_def_pattern,
                &add_synth_pattern,
                &mut names,
            )?;
        }
    }

    Ok(names.into_iter().collect())
}

fn collect_synth_names_from_dir(
    root: &Path,
    synth_def_pattern: &Regex,
    add_synth_pattern: &Regex,
    names: &mut BTreeSet<String>,
) -> Result<(), SoundsError> {
    for entry in fs::read_dir(root)
        .map_err(|error| SoundsError::new(format!("failed to read {}: {error}", root.display())))?
    {
        let entry = entry.map_err(|error| {
            SoundsError::new(format!(
                "failed to inspect entries in {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_synth_names_from_dir(&path, synth_def_pattern, add_synth_pattern, names)?;
            continue;
        }

        let extension = path.extension().and_then(|ext| ext.to_str());
        if !matches!(extension, Some("scd" | "sc")) {
            continue;
        }

        let content = fs::read_to_string(&path).map_err(|error| {
            SoundsError::new(format!("failed to read {}: {error}", path.display()))
        })?;
        for captures in synth_def_pattern.captures_iter(&content) {
            names.insert(captures[1].to_string());
        }
        for captures in add_synth_pattern.captures_iter(&content) {
            names.insert(captures[1].to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("{prefix}-{unique}-{counter}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn collects_sample_and_synth_names_from_overrides() {
        let samples_root = temp_dir("fin-samples");
        fs::create_dir(samples_root.join("bd")).expect("sample dir should exist");
        fs::create_dir(samples_root.join("808sd")).expect("sample dir should exist");
        fs::create_dir(samples_root.join(".git")).expect("hidden dir should exist");

        let superdirt_root = temp_dir("fin-superdirt");
        let synths_dir = superdirt_root.join("synths");
        fs::create_dir_all(&synths_dir).expect("synth dir should exist");
        fs::write(
            synths_dir.join("default-synths.scd"),
            "SynthDef(\\superpiano,{})\n~dirt.soundLibrary.addSynth(\\from, (play: {}));\n",
        )
        .expect("synth source should be written");

        unsafe {
            env::set_var(FIN_DIRT_SAMPLES_ROOT, &samples_root);
            env::set_var(FIN_SUPERDIRT_ROOT, &superdirt_root);
        }
        let report = load_sounds_report().expect("report should load");
        unsafe {
            env::remove_var(FIN_DIRT_SAMPLES_ROOT);
            env::remove_var(FIN_SUPERDIRT_ROOT);
        }

        assert_eq!(
            report.sample_names,
            vec!["808sd".to_string(), "bd".to_string()]
        );
        assert!(report.synth_names.contains(&"superpiano".to_string()));
        assert!(report.synth_names.contains(&"from".to_string()));
    }
}
