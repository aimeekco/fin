use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::SystemTime;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug)]
pub struct FileChangeWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    target_path: PathBuf,
    target_name: Option<std::ffi::OsString>,
    last_stamp: Option<FileStamp>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WatchPoll {
    pub changed: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileChangeWatcher {
    pub fn new(target_path: &Path) -> Result<Self, String> {
        let absolute_target =
            std::fs::canonicalize(target_path).unwrap_or_else(|_| target_path.to_path_buf());
        let watch_root = absolute_target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let target_name = absolute_target.file_name().map(OsStr::to_os_string);
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = sender.send(result);
        })
        .map_err(|error| format!("failed to create file watcher: {error}"))?;

        watcher
            .watch(&watch_root, RecursiveMode::NonRecursive)
            .map_err(|error| format!("failed to watch {}: {error}", watch_root.display()))?;

        let last_stamp = read_file_stamp(&absolute_target);

        Ok(Self {
            watcher,
            receiver,
            target_path: absolute_target,
            target_name,
            last_stamp,
        })
    }

    pub fn poll(&mut self) -> WatchPoll {
        let mut poll = WatchPoll::default();

        loop {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => {
                    if is_relevant_event(&event, &self.target_path, self.target_name.as_deref()) {
                        poll.changed = true;
                    }
                }
                Ok(Err(error)) => poll.errors.push(error.to_string()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    poll.errors
                        .push("file watcher disconnected unexpectedly".to_string());
                    break;
                }
            }
        }

        let current_stamp = read_file_stamp(&self.target_path);
        if current_stamp != self.last_stamp {
            poll.changed = true;
            self.last_stamp = current_stamp;
        }

        poll
    }

    pub fn watched_path(&self) -> &Path {
        &self.target_path
    }

    pub fn is_active(&self) -> bool {
        let _ = &self.watcher;
        true
    }
}

fn is_relevant_event(event: &Event, target_path: &Path, target_name: Option<&OsStr>) -> bool {
    if !is_mutating_kind(&event.kind) {
        return false;
    }

    event
        .paths
        .iter()
        .any(|path| path_matches_target(path, target_path, target_name))
}

fn is_mutating_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Other
    )
}

fn path_matches_target(path: &Path, target_path: &Path, target_name: Option<&OsStr>) -> bool {
    if path == target_path {
        return true;
    }

    if let Ok(canonical_path) = std::fs::canonicalize(path) {
        if canonical_path == target_path {
            return true;
        }
    }

    match (path.file_name(), target_name) {
        (Some(path_name), Some(target_name)) => path_name == target_name,
        _ => false,
    }
}

fn read_file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{WatchPoll, is_relevant_event, path_matches_target};
    use notify::event::{CreateKind, ModifyKind};
    use notify::{Event, EventKind};
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn exact_path_match_is_relevant() {
        let target = Path::new("/tmp/basic.metl");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![target.to_path_buf()],
            attrs: Default::default(),
        };

        assert!(is_relevant_event(
            &event,
            target,
            Some(OsStr::new("basic.metl"))
        ));
    }

    #[test]
    fn filename_match_handles_atomic_replace_paths() {
        assert!(
            path_matches_target(
                Path::new("/tmp/.basic.metl.swp"),
                Path::new("/tmp/basic.metl"),
                Some(OsStr::new("basic.metl"))
            ) == false
        );
        assert!(path_matches_target(
            Path::new("/tmp/basic.metl"),
            Path::new("/private/tmp/basic.metl"),
            Some(OsStr::new("basic.metl"))
        ));
    }

    #[test]
    fn non_mutating_events_are_ignored() {
        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Any),
            paths: vec![Path::new("/tmp/basic.metl").to_path_buf()],
            attrs: Default::default(),
        };

        assert!(!is_relevant_event(
            &event,
            Path::new("/tmp/basic.metl"),
            Some(OsStr::new("basic.metl"))
        ));
    }

    #[test]
    fn watch_poll_defaults_to_empty() {
        assert_eq!(
            WatchPoll::default(),
            WatchPoll {
                changed: false,
                errors: vec![]
            }
        );
    }

    #[test]
    fn create_events_are_relevant() {
        let event = Event {
            kind: EventKind::Create(CreateKind::Any),
            paths: vec![Path::new("/tmp/basic.metl").to_path_buf()],
            attrs: Default::default(),
        };

        assert!(is_relevant_event(
            &event,
            Path::new("/tmp/basic.metl"),
            Some(OsStr::new("basic.metl"))
        ));
    }
}
