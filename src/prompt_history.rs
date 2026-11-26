//! Local agent prompt history. Disk I/O runs on its own thread, off the input path.
use crate::app::Message;
use serde_json::json;
use std::{
    env, fs,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::PathBuf,
    sync::mpsc::{self, Sender},
    thread,
};
use tao::event_loop::EventLoopProxy;

const MAX_ENTRIES: usize = 500;
const MAX_ENTRY_CHARS: usize = 4_000;
const MAX_BYTES: u64 = 4 * 1024 * 1024;

pub enum Request {
    Load,
    Record { entry: String, seq: u64 },
}

pub fn start(proxy: EventLoopProxy<Message>) -> Sender<Request> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let home = env::home_dir().unwrap_or_default();
        let path = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join("Library/Application Support"))
            .join("minterm/prompt-history.json");
        let mut store = Store::new(path);
        while let Ok(request) = rx.recv() {
            match request {
                Request::Load => {
                    let entries = store.load();
                    let _ = proxy.send_event(Message::PromptHistory(json!({
                        "entries": entries,
                        "seq": 0,
                    })));
                }
                Request::Record { entry, seq } => {
                    let entries = store.record(&entry);
                    let _ = proxy.send_event(Message::PromptHistory(json!({
                        "entries": entries,
                        "seq": seq,
                    })));
                }
            }
        }
    });
    tx
}

fn sanitize(entry: &str) -> Option<String> {
    if entry.trim().is_empty() || entry.contains('\0') {
        return None;
    }
    if entry.chars().count() > MAX_ENTRY_CHARS {
        return Some(entry.chars().take(MAX_ENTRY_CHARS).collect());
    }
    Some(entry.to_owned())
}

fn normalize(entries: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for raw in entries {
        let Some(entry) = sanitize(&raw) else {
            continue;
        };
        if let Some(pos) = normalized.iter().position(|existing| existing == &entry) {
            normalized.remove(pos);
        }
        normalized.push(entry);
    }
    if normalized.len() > MAX_ENTRIES {
        normalized.drain(..normalized.len() - MAX_ENTRIES);
    }
    normalized
}

struct Store {
    path: PathBuf,
    entries: Vec<String>,
}

impl Store {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
        }
    }

    fn read_disk(&self) -> Result<Vec<String>, std::io::Error> {
        let file = fs::File::open(&self.path)?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(std::io::Error::other("prompt history too large"));
        }
        let raw: Vec<String> = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        Ok(normalize(raw))
    }

    fn load(&mut self) -> Vec<String> {
        if let Ok(entries) = self.read_disk() {
            self.entries = entries;
        }
        self.entries.clone()
    }

    fn record(&mut self, entry: &str) -> Vec<String> {
        let Some(entry) = sanitize(entry) else {
            return self.entries.clone();
        };
        if self.merge_save(&entry).is_err() {
            if let Some(pos) = self.entries.iter().position(|existing| existing == &entry) {
                self.entries.remove(pos);
            }
            self.entries.push(entry);
            if self.entries.len() > MAX_ENTRIES {
                self.entries.drain(..self.entries.len() - MAX_ENTRIES);
            }
        }
        self.entries.clone()
    }

    fn merge_save(&mut self, entry: &str) -> std::io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("missing parent"))?;
        fs::create_dir_all(parent)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.path.with_extension("lock"))?;
        // Hold a sidecar lock across read/merge/atomic replace so separate
        // Minterm windows do not overwrite each other's prompt history.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut latest = match self.read_disk() {
            Ok(entries) => entries,
            Err(_) => self.entries.clone(),
        };
        if let Some(pos) = latest.iter().position(|existing| existing == entry) {
            latest.remove(pos);
        }
        latest.push(entry.to_owned());
        if latest.len() > MAX_ENTRIES {
            latest.drain(..latest.len() - MAX_ENTRIES);
        }
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut file, &latest)?;
        file.flush()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        self.entries = latest;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn records_deduplicates_persists_and_merges_windows() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state/prompt-history.json");
        let mut first = Store::new(path.clone());
        let mut second = Store::new(path.clone());

        assert!(first.load().is_empty());
        assert_eq!(first.record("   \n  "), Vec::<String>::new());
        assert_eq!(first.record("first task"), vec!["first task"]);
        assert_eq!(
            second.record("second\nmultiline task"),
            vec!["first task", "second\nmultiline task"]
        );
        assert_eq!(
            first.record("first task"),
            vec!["second\nmultiline task", "first task"]
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let mut reloaded = Store::new(path);
        assert_eq!(
            reloaded.load(),
            vec!["second\nmultiline task", "first task"]
        );
    }

    #[test]
    fn bounds_entries_and_recovers_from_corrupt_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prompt-history.json");
        fs::write(&path, b"not-json").unwrap();
        let mut store = Store::new(path.clone());
        assert!(store.load().is_empty());
        for i in 0..(MAX_ENTRIES + 5) {
            store.record(&format!("prompt {i}"));
        }
        let loaded = Store::new(path).load();
        assert_eq!(loaded.len(), MAX_ENTRIES);
        assert_eq!(loaded.first().map(String::as_str), Some("prompt 5"));
        assert_eq!(
            loaded.last().map(String::as_str),
            Some(format!("prompt {}", MAX_ENTRIES + 4).as_str())
        );
    }
}
