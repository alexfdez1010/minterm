//! Local, rolling directory visits. Filesystem work never runs on the input path.
use crate::{app::Message, sessions::process_cwd};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tao::event_loop::EventLoopProxy;

const WEEK: u64 = 7 * 24 * 60 * 60;
const MAX_VISITS: usize = 4_096;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub enum Request {
    Shells(Vec<(u32, i32)>),
    List,
    Open {
        id: u32,
        path: PathBuf,
        cols: u16,
        rows: u16,
    },
}

pub fn start(proxy: EventLoopProxy<Message>) -> Sender<Request> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let home = env::home_dir().unwrap_or_default();
        let path = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join("Library/Application Support"))
            .join("minterm/projects.json");
        let mut history = History::load(path, home);
        let mut shells = Vec::new();
        loop {
            let request = match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(request) => Some(request),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if let Some(Request::Shells(current)) = &request {
                shells.clone_from(current);
            }
            history.sample(&shells, now());
            match request {
                Some(Request::List) => {
                    history.refresh(now());
                    let entries = history.ranked(now());
                    let _ = proxy.send_event(Message::Projects(json!({
                        "entries":entries, "error":history.error
                    })));
                }
                Some(Request::Open {
                    id,
                    path,
                    cols,
                    rows,
                }) => {
                    let directory = history.resolve(&path, now());
                    let _ = proxy.send_event(Message::ProjectDirectory {
                        id,
                        directory,
                        cols,
                        rows,
                    });
                }
                _ => {}
            }
        }
    });
    tx
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Deserialize, Serialize)]
struct Visit {
    path: PathBuf,
    at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    path: PathBuf,
    name: String,
    display_path: String,
    visits: usize,
    last_visit: u64,
}

struct History {
    path: PathBuf,
    home: PathBuf,
    visits: Vec<Visit>,
    pending: Vec<Visit>,
    // Cache only the last observed directory of each live shell, never a tree scan.
    current: HashMap<u32, (PathBuf, Option<PathBuf>)>,
    error: Option<String>,
}

impl History {
    fn load(path: PathBuf, home: PathBuf) -> Self {
        let loaded = (|| {
            let file = fs::File::open(&path)?;
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(std::io::Error::other("history too large"));
            }
            serde_json::from_slice::<Vec<Visit>>(&bytes).map_err(std::io::Error::other)
        })();
        let (visits, error) = match loaded {
            Ok(visits) => (visits, None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
            Err(_) => (
                Vec::new(),
                Some("Could not read project history. New visits will rebuild it.".into()),
            ),
        };
        Self {
            path,
            home,
            visits,
            pending: Vec::new(),
            current: HashMap::new(),
            error,
        }
    }

    fn prune(&mut self, now: u64) -> bool {
        let count = self.visits.len();
        self.visits.retain(|v| {
            v.at > now.saturating_sub(WEEK)
                && v.at <= now
                && v.path.is_absolute()
                && v.path.as_os_str().len() < 1024
                && v.path != self.home
                && v.path.parent().is_some()
        });
        if self.visits.len() > MAX_VISITS {
            self.visits.drain(..self.visits.len() - MAX_VISITS);
        }
        self.pending
            .retain(|v| v.at > now.saturating_sub(WEEK) && v.at <= now);
        if self.pending.len() > MAX_VISITS {
            self.pending.drain(..self.pending.len() - MAX_VISITS);
        }
        self.visits.len() != count
    }

    fn observe(&mut self, id: u32, cwd: PathBuf, now: u64) -> bool {
        if self
            .current
            .get(&id)
            .is_some_and(|(previous, _)| previous == &cwd)
        {
            return false;
        }
        let project = project_root(&cwd, &self.home);
        let previous = self.current.insert(id, (cwd, project.clone()));
        if let Some(path) = project
            && previous.as_ref().and_then(|(_, path)| path.as_ref()) != Some(&path)
        {
            let visit = Visit { path, at: now };
            self.visits.push(visit.clone());
            self.pending.push(visit);
            return true;
        }
        false
    }

    fn sample(&mut self, shells: &[(u32, i32)], now: u64) {
        self.current
            .retain(|id, _| shells.iter().any(|(live, _)| id == live));
        let mut changed = self.prune(now);
        for &(id, pid) in shells {
            if let Some(cwd) = process_cwd(pid) {
                changed |= self.observe(id, cwd, now);
            }
        }
        changed |= self.prune(now);
        if changed || !self.pending.is_empty() {
            self.error = self.merge_save(now).err().map(|_| {
                "Could not save project history. Visits are available until Minterm quits.".into()
            });
        }
    }

    fn refresh(&mut self, now: u64) {
        let latest = Self::load(self.path.clone(), self.home.clone());
        if latest.error.is_none() {
            self.visits = latest.visits;
            self.visits.extend_from_slice(&self.pending);
            self.prune(now);
        }
    }

    fn merge_save(&mut self, now: u64) -> std::io::Result<()> {
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
        // Separate Minterm windows can be separate processes. Hold a sidecar lock
        // across read/merge/atomic replace so one window cannot erase another's visits.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut latest = Self::load(self.path.clone(), self.home.clone());
        if latest.error.is_some() {
            latest.visits.clone_from(&self.visits);
        } else {
            latest.visits.extend_from_slice(&self.pending);
        }
        latest.prune(now);
        latest.save()?;
        self.visits = latest.visits;
        self.pending.clear();
        Ok(())
    }

    fn ranked(&self, now: u64) -> Vec<Project> {
        let mut grouped: HashMap<&Path, (usize, u64)> = HashMap::new();
        for visit in &self.visits {
            if visit.at <= now.saturating_sub(WEEK) || visit.at > now {
                continue;
            }
            let entry = grouped.entry(&visit.path).or_default();
            entry.0 += 1;
            entry.1 = entry.1.max(visit.at);
        }
        let mut projects: Vec<_> = grouped
            .into_iter()
            .filter_map(|(path, (visits, last_visit))| {
                if !path.is_dir() {
                    return None;
                }
                let name = path.file_name()?.to_str()?.to_owned();
                let display_path = match path.strip_prefix(&self.home) {
                    Ok(rest) => Path::new("~").join(rest).display().to_string(),
                    Err(_) => path.display().to_string(),
                };
                Some(Project {
                    path: path.to_owned(),
                    name,
                    display_path,
                    visits,
                    last_visit,
                })
            })
            .collect();
        projects.sort_by(|a, b| {
            b.visits
                .cmp(&a.visits)
                .then_with(|| b.last_visit.cmp(&a.last_visit))
                .then_with(|| a.path.cmp(&b.path))
        });
        projects
    }

    fn resolve(&self, path: &Path, now: u64) -> Result<PathBuf, String> {
        if self
            .visits
            .iter()
            .any(|v| v.path == path && v.at > now.saturating_sub(WEEK) && v.at <= now)
            && path.is_absolute()
            && path.is_dir()
        {
            Ok(path.to_owned())
        } else {
            Err(
                "This project folder is no longer available. Open Projects to refresh the list."
                    .into(),
            )
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("missing parent"))?;
        fs::create_dir_all(parent)?;
        // NamedTempFile is private (0600); replacement is atomic on the same volume.
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut file, &self.visits)?;
        file.flush()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
}

fn project_root(cwd: &Path, home: &Path) -> Option<PathBuf> {
    let cwd = cwd.canonicalize().ok()?;
    if cwd == home || cwd.parent().is_none() || cwd.to_str().is_none() {
        return None;
    }
    for ancestor in cwd.ancestors() {
        if ancestor == home || ancestor.parent().is_none() {
            break;
        }
        // A worktree's .git is a file; ordinary repositories use a directory.
        if ancestor.join(".git").exists() {
            return Some(ancestor.to_owned());
        }
    }
    Some(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn visits_count_entries_not_samples_or_subfolders() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let repo = home.join("minterm");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir(repo.join("ui")).unwrap();
        let other = home.join("Other folder ' $(literal)");
        fs::create_dir(&other).unwrap();
        let mut h = History::load(home.join("state/projects.json"), home.clone());
        assert!(h.observe(1, repo.clone(), WEEK));
        assert!(!h.observe(1, repo.clone(), WEEK + 1));
        assert!(!h.observe(1, repo.join("ui"), WEEK + 2));
        assert!(h.observe(2, repo.clone(), WEEK + 3));
        assert!(h.observe(1, other.clone(), WEEK + 4));
        assert!(h.observe(1, repo.clone(), WEEK + 5));
        let entries = h.ranked(WEEK + 5);
        assert_eq!(entries[0].path, repo);
        assert_eq!(entries[0].visits, 3);
        assert_eq!(h.resolve(&other, WEEK + 5).unwrap(), other);
        h.save().unwrap();
        assert_eq!(
            fs::metadata(&h.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let loaded = History::load(h.path.clone(), home);
        assert_eq!(loaded.ranked(WEEK + 5)[0].visits, 3);
        fs::remove_dir(other).unwrap();
        assert_eq!(loaded.ranked(WEEK + 5).len(), 1);
    }

    #[test]
    fn expiry_recency_missing_folders_and_corrupt_history() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let mut h = History::load(home.join("projects.json"), home.clone());
        for (at, name) in [(10, "expired"), (11, "older"), (12, "recent")] {
            let path = home.join(name);
            fs::create_dir(&path).unwrap();
            h.visits.push(Visit { path, at });
        }
        h.prune(WEEK + 10);
        let entries = h.ranked(WEEK + 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "recent");
        assert!(h.resolve(&home.join("expired"), WEEK + 10).is_err());
        assert!(!h.observe(1, home.clone(), WEEK + 10));
        assert!(!h.observe(1, PathBuf::from("/"), WEEK + 10));
        fs::write(&h.path, b"broken").unwrap();
        let mut recovered = History::load(h.path, home.clone());
        assert!(recovered.error.is_some());
        assert!(recovered.observe(1, home.join("recent"), WEEK + 10));
        recovered.save().unwrap();
        assert_eq!(History::load(recovered.path, home).visits.len(), 1);
    }

    #[test]
    fn worktrees_aliases_and_bounded_history() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let worktree = home.join("worktree");
        fs::create_dir_all(worktree.join("src/deep")).unwrap();
        fs::write(worktree.join(".git"), "gitdir: elsewhere").unwrap();
        let alias = home.join("alias");
        symlink(&worktree, &alias).unwrap();
        assert_eq!(
            project_root(&alias.join("src/deep"), &home),
            Some(worktree.clone())
        );
        let mut h = History::load(home.join("projects.json"), home);
        h.visits = vec![
            Visit {
                path: worktree,
                at: WEEK
            };
            MAX_VISITS + 1
        ];
        assert!(h.prune(WEEK));
        assert_eq!(h.visits.len(), MAX_VISITS);
    }

    #[test]
    fn independent_windows_merge_visits_and_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let path = home.join("state/projects.json");
        let repo = home.join("minterm");
        fs::create_dir(&repo).unwrap();
        let mut first = History::load(path.clone(), home.clone());
        let mut second = History::load(path, home);
        first.observe(1, repo.clone(), WEEK);
        second.observe(1, repo.clone(), WEEK + 1);
        first.merge_save(WEEK + 1).unwrap();
        second.merge_save(WEEK + 1).unwrap();
        assert_eq!(second.ranked(WEEK + 1)[0].visits, 2);
        first.refresh(WEEK + 1);
        assert_eq!(first.ranked(WEEK + 1)[0].visits, 2);
        first.observe(1, repo.clone(), WEEK + 2);
        assert_eq!(first.ranked(WEEK + 2)[0].visits, 2);
        assert!(first.pending.is_empty());
    }
}
