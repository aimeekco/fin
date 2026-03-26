use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

const FIN_DIRT_SAMPLES_ROOT: &str = "FIN_DIRT_SAMPLES_ROOT";
const FIN_SUPERDIRT_ROOT: &str = "FIN_SUPERDIRT_ROOT";
const AUDIO_FILE_EXTENSIONS: &[&str] =
    &["wav", "wave", "aif", "aiff", "flac", "ogg", "mp3", "au"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundsReport {
    pub samples_root: Option<PathBuf>,
    pub sample_banks: Vec<SampleBank>,
    pub superdirt_root: Option<PathBuf>,
    pub synths: Vec<SynthSound>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleBank {
    pub name: String,
    pub description: String,
    pub samples: Vec<SampleEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleEntry {
    pub index: usize,
    pub file_name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthSound {
    pub name: String,
    pub description: String,
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

    let sample_banks = match &samples_root {
        Some(root) => collect_sample_banks(root)?,
        None => Vec::new(),
    };
    let synths = match &superdirt_root {
        Some(root) => collect_synths(root)?,
        None => Vec::new(),
    };

    if samples_root.is_none() && superdirt_root.is_none() {
        return Err(SoundsError::new(
            "could not locate SuperDirt samples or quark install. Set `FIN_DIRT_SAMPLES_ROOT` and/or `FIN_SUPERDIRT_ROOT` if needed.",
        ));
    }

    Ok(SoundsReport {
        samples_root,
        sample_banks,
        superdirt_root,
        synths,
    })
}

pub fn format_sounds_report(report: &SoundsReport) -> String {
    let mut lines = Vec::new();

    if let Some(root) = &report.samples_root {
        lines.push(format!("samples_root={}", root.display()));
        lines.push(format!("sample_count={}", report.sample_banks.len()));
        lines.extend(
            report
                .sample_banks
                .iter()
                .map(|bank| format!("sample {}  {}", bank.name, bank.description)),
        );
    } else {
        lines.push("samples_root=not_found".to_string());
    }

    if let Some(root) = &report.superdirt_root {
        lines.push(format!("superdirt_root={}", root.display()));
        lines.push(format!("synth_count={}", report.synths.len()));
        lines.extend(
            report
                .synths
                .iter()
                .map(|synth| format!("synth {}  {}", synth.name, synth.description)),
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

fn collect_sample_banks(root: &Path) -> Result<Vec<SampleBank>, SoundsError> {
    let entries = fs::read_dir(root)
        .map_err(|error| SoundsError::new(format!("failed to read {}: {error}", root.display())))?;
    let mut banks = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            SoundsError::new(format!(
                "failed to inspect entries in {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        if let Some(bank) = collect_sample_bank(&path, name)? {
            banks.push(bank);
        }
    }

    banks.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(banks)
}

fn collect_sample_bank(root: &Path, name: &str) -> Result<Option<SampleBank>, SoundsError> {
    let mut audio_files = Vec::new();
    collect_sample_audio_files(root, &mut audio_files)?;
    if audio_files.is_empty() {
        return Ok(None);
    }

    audio_files.sort();
    let samples = audio_files
        .into_iter()
        .enumerate()
        .map(|(index, relative_path)| SampleEntry {
            index,
            description: describe_sample_file(name, &relative_path),
            file_name: relative_path.display().to_string(),
        })
        .collect::<Vec<_>>();

    Ok(Some(SampleBank {
        name: name.to_string(),
        description: describe_sample_bank(name, &samples),
        samples,
    }))
}

fn collect_sample_audio_files(root: &Path, audio_files: &mut Vec<PathBuf>) -> Result<(), SoundsError> {
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
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if !AUDIO_FILE_EXTENSIONS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            continue;
        }

        let relative = path.strip_prefix(root).map_err(|error| {
            SoundsError::new(format!(
                "failed to strip sample root prefix from {}: {error}",
                path.display()
            ))
        })?;
        audio_files.push(relative.to_path_buf());
    }

    Ok(())
}

fn collect_synths(root: &Path) -> Result<Vec<SynthSound>, SoundsError> {
    let mut registered_names = BTreeSet::new();
    let mut synth_def_names = BTreeSet::new();
    let synth_def_pattern = Regex::new(r#"SynthDef\(\s*\\([A-Za-z0-9_]+)"#)
        .map_err(|error| SoundsError::new(format!("invalid synth regex: {error}")))?;
    let add_synth_pattern = Regex::new(r#"addSynth\(\s*\\([A-Za-z0-9_]+)"#)
        .map_err(|error| SoundsError::new(format!("invalid addSynth regex: {error}")))?;

    for relative in [Path::new("synths"), Path::new("library"), Path::new("hacks")] {
        let directory = root.join(relative);
        if directory.is_dir() {
            collect_synth_names_from_dir(
                &directory,
                &synth_def_pattern,
                &add_synth_pattern,
                &mut synth_def_names,
                &mut registered_names,
            )?;
        }
    }

    let mut all_names = synth_def_names;
    all_names.extend(registered_names.iter().cloned());
    Ok(all_names
        .into_iter()
        .map(|name| SynthSound {
            description: describe_synth(&name, registered_names.contains(&name)),
            name,
        })
        .collect())
}

fn collect_synth_names_from_dir(
    root: &Path,
    synth_def_pattern: &Regex,
    add_synth_pattern: &Regex,
    synth_def_names: &mut BTreeSet<String>,
    registered_names: &mut BTreeSet<String>,
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
            collect_synth_names_from_dir(
                &path,
                synth_def_pattern,
                add_synth_pattern,
                synth_def_names,
                registered_names,
            )?;
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
            synth_def_names.insert(captures[1].to_string());
        }
        for captures in add_synth_pattern.captures_iter(&content) {
            registered_names.insert(captures[1].to_string());
        }
    }

    Ok(())
}

fn describe_sample(name: &str) -> String {
    if let Some(description) = describe_drum_code(name) {
        return description.to_string();
    }

    if let Some((prefix, suffix)) = split_numeric_prefix(name) {
        if let Some(base) = describe_drum_code(suffix) {
            return format!("{prefix}-style {base}");
        }
    }

    if name.len() <= 3 {
        return "sample bank".to_string();
    }

    format!("sample bank for `{name}`")
}

fn describe_sample_bank(name: &str, samples: &[SampleEntry]) -> String {
    let base = describe_sample(name);
    if base != "sample bank" {
        return base;
    }

    let joined = samples
        .iter()
        .take(12)
        .map(|sample| sample.description.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    let drum_hits = count_keywords(
        &joined,
        &[
            "kick",
            "snare",
            "hi-hat",
            "crash",
            "ride",
            "percussion",
            "tom",
            "clap",
            "rimshot",
        ],
    );
    let melodic_hits = count_keywords(
        &joined,
        &["bass", "bassline", "melody", "chord", "piano", "note", "rhythm"],
    );
    let texture_hits = count_keywords(
        &joined,
        &["rise", "glass", "fan", "micro", "texture", "noise", "fx"],
    );

    if drum_hits >= 3 {
        "drum kit / percussion bank".to_string()
    } else if melodic_hits >= 2 && texture_hits >= 1 {
        "melodic / texture bank".to_string()
    } else if melodic_hits >= 2 {
        "melodic sample bank".to_string()
    } else if texture_hits >= 2 {
        "texture / fx bank".to_string()
    } else {
        base
    }
}

fn count_keywords(text: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|keyword| text.contains(**keyword)).count()
}

fn describe_synth(name: &str, is_registered: bool) -> String {
    match name {
        "superfm" => "FM synth".to_string(),
        "superpiano" => "piano synth".to_string(),
        "superchip" => "chip synth".to_string(),
        "supermandolin" => "mandolin-style synth".to_string(),
        "supergong" => "gong-like synth".to_string(),
        _ if is_registered => format!("registered SuperDirt synth `{name}`"),
        _ => format!("SynthDef-backed synth `{name}`"),
    }
}

fn describe_drum_code(name: &str) -> Option<&'static str> {
    match name {
        "bd" => Some("bass drum / kick"),
        "sd" => Some("snare drum"),
        "hh" => Some("hi-hat"),
        "cp" => Some("clap"),
        "rs" => Some("rimshot"),
        "lt" => Some("low tom"),
        "mt" => Some("mid tom"),
        "ht" => Some("high tom"),
        "cb" => Some("cowbell"),
        "hc" => Some("hand clap"),
        "oh" => Some("open hi-hat"),
        "ch" => Some("closed hi-hat"),
        _ => None,
    }
}

fn split_numeric_prefix(name: &str) -> Option<(&str, &str)> {
    let prefix_len = name.bytes().take_while(u8::is_ascii_digit).count();
    if prefix_len == 0 || prefix_len == name.len() {
        return None;
    }

    Some(name.split_at(prefix_len))
}

fn describe_sample_file(bank_name: &str, relative_path: &Path) -> String {
    let stem = relative_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let mut value = trim_noise_prefix(&stem.to_ascii_lowercase());
    let bank_name = bank_name.to_ascii_lowercase();

    if let Some(stripped) = value.strip_prefix(&bank_name) {
        value = trim_noise_prefix(stripped);
    }

    let normalized = normalize_sample_label(&value);
    if normalized.is_empty() {
        stem.to_string()
    } else {
        normalized
    }
}

fn trim_noise_prefix(input: &str) -> String {
    input
        .trim_start_matches(|ch: char| ch.is_ascii_digit() || matches!(ch, '_' | '-' | ' '))
        .to_string()
}

fn normalize_sample_label(input: &str) -> String {
    let compact = input.trim().to_ascii_lowercase();
    if compact.is_empty() {
        return String::new();
    }

    let replaced = match compact.as_str() {
        "closedhh" => "closed hi-hat".to_string(),
        "openhh" => "open hi-hat".to_string(),
        "fanbass" => "fan bass".to_string(),
        "microsound" => "micro sound".to_string(),
        other => other
            .replace("closedhh", "closed hi-hat")
            .replace("openhh", "open hi-hat")
            .replace("hh", " hi-hat")
            .replace("perc", "percussion ")
            .replace("fanbass", "fan bass")
            .replace("microsound", "micro sound"),
    };

    let separated = separate_alnum_runs(
        &replaced
            .chars()
            .map(|ch| if matches!(ch, '_' | '-' | '.') { ' ' } else { ch })
            .collect::<String>(),
    );
    collapse_spaces(&separated)
}

fn separate_alnum_runs(input: &str) -> String {
    let mut output = String::new();
    let mut previous: Option<char> = None;

    for current in input.chars() {
        if let Some(last) = previous {
            let should_split = (last.is_ascii_alphabetic() && current.is_ascii_digit())
                || (last.is_ascii_digit() && current.is_ascii_alphabetic());
            if should_split && !output.ends_with(' ') {
                output.push(' ');
            }
        }
        output.push(current);
        previous = Some(current);
    }

    output
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
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
        fs::write(samples_root.join("bd").join("0.wav"), "").expect("audio sample should exist");
        fs::create_dir(samples_root.join("808sd")).expect("sample dir should exist");
        fs::write(samples_root.join("808sd").join("001_snare1.AIFF"), "")
            .expect("audio sample should exist");
        fs::create_dir(samples_root.join("broken")).expect("broken sample dir should exist");
        fs::write(samples_root.join("broken").join("README.txt"), "not audio")
            .expect("non-audio file should exist");
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
            report
                .sample_banks
                .iter()
                .map(|bank| bank.name.clone())
                .collect::<Vec<_>>(),
            vec!["808sd".to_string(), "bd".to_string()]
        );
        assert_eq!(
            report
                .synths
                .iter()
                .map(|synth| synth.name.clone())
                .collect::<Vec<_>>(),
            vec!["from".to_string(), "superpiano".to_string()]
        );
        assert_eq!(report.sample_banks[0].samples[0].index, 0);
        assert_eq!(report.sample_banks[0].samples[0].description, "snare 1");
    }

    #[test]
    fn formats_sound_lines_with_descriptions() {
        let report = SoundsReport {
            samples_root: Some(PathBuf::from("/tmp/samples")),
            sample_banks: vec![
                SampleBank {
                    name: "808sd".to_string(),
                    description: "808-style snare drum".to_string(),
                    samples: vec![],
                },
                SampleBank {
                    name: "bd".to_string(),
                    description: "bass drum / kick".to_string(),
                    samples: vec![],
                },
                SampleBank {
                    name: "tablex".to_string(),
                    description: "sample bank for `tablex`".to_string(),
                    samples: vec![],
                },
            ],
            superdirt_root: Some(PathBuf::from("/tmp/superdirt")),
            synths: vec![SynthSound {
                name: "from".to_string(),
                description: "registered SuperDirt synth `from`".to_string(),
            }],
        };

        let formatted = format_sounds_report(&report);

        assert!(formatted.contains("sample 808sd  808-style snare drum"));
        assert!(formatted.contains("sample bd  bass drum / kick"));
        assert!(formatted.contains("sample tablex  sample bank for `tablex`"));
        assert!(formatted.contains("synth from  registered SuperDirt synth `from`"));
    }

    #[test]
    fn infers_bank_descriptions_from_sample_filenames() {
        let ab_samples = vec![
            SampleEntry {
                index: 0,
                file_name: "000_ab2closedhh.wav".to_string(),
                description: "closed hi-hat".to_string(),
            },
            SampleEntry {
                index: 1,
                file_name: "001_ab2crash.wav".to_string(),
                description: "crash".to_string(),
            },
            SampleEntry {
                index: 2,
                file_name: "004_ab2kick1.wav".to_string(),
                description: "kick 1".to_string(),
            },
            SampleEntry {
                index: 3,
                file_name: "010_ab2snare1.wav".to_string(),
                description: "snare 1".to_string(),
            },
        ];
        let ade_samples = vec![
            SampleEntry {
                index: 0,
                file_name: "000_011112-bassline.wav".to_string(),
                description: "bassline".to_string(),
            },
            SampleEntry {
                index: 1,
                file_name: "001_011112-melody.wav".to_string(),
                description: "melody".to_string(),
            },
            SampleEntry {
                index: 2,
                file_name: "006_glass.wav".to_string(),
                description: "glass".to_string(),
            },
        ];

        assert_eq!(
            describe_sample_bank("ab", &ab_samples),
            "drum kit / percussion bank"
        );
        assert_eq!(
            describe_sample_bank("ade", &ade_samples),
            "melodic / texture bank"
        );
    }
}
