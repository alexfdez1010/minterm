use crate::{
    agents, inspect, projects,
    sessions::{SessionEvent, Sessions},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    thread,
};
use tao::event_loop::EventLoopProxy;
use wry::WebView;

/// Messages accepted from the embedded, local page.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    Ready,
    AgentPreferences,
    Open {
        id: u32,
        from: Option<u32>,
        cols: u16,
        rows: u16,
    },
    OpenAgent {
        id: u32,
        from: u32,
        agent: String,
        prompt: String,
        cols: u16,
        rows: u16,
    },
    Write {
        id: u32,
        data: String,
    },
    Binary {
        id: u32,
        data: String,
    },
    Resize {
        id: u32,
        cols: u16,
        rows: u16,
    },
    Ack {
        id: u32,
    },
    Close {
        id: u32,
        #[serde(default)]
        force: bool,
    },
    Quit {
        #[serde(default)]
        force: bool,
    },
    Context {
        id: u32,
    },
    Services,
    StopListener {
        listener: inspect::Port,
    },
    SaveInspector {
        state: crate::preferences::InspectorPreferences,
    },
    SaveSplitView {
        enabled: bool,
    },
    Agents,
    Projects,
    OpenProject {
        id: u32,
        path: PathBuf,
        cols: u16,
        rows: u16,
    },
    Drag,
}

pub enum Message {
    Preferences,
    Action(Action),
    Shell(SessionEvent),
    Menu(String),
    Context(u32, Value),
    Services(Value),
    ListenerStopped(Option<String>),
    Agents(Value),
    Projects(Value),
    ProjectDirectory {
        id: u32,
        directory: Result<PathBuf, String>,
        cols: u16,
        rows: u16,
    },
}

struct Companion {
    id: u32,
    live: bool,
}

pub struct App {
    view: WebView,
    proxy: EventLoopProxy<Message>,
    sessions: Sessions,
    projects: Sender<projects::Request>,
    opening_projects: HashSet<u32>,
    // Each logical tab can own a shell and a second PTY running a live agent.
    companions: HashMap<u32, Companion>,
    start_dir: PathBuf,
    contexts: HashSet<u32>,
    last_dirs: HashMap<u32, PathBuf>,
    scanning_services: bool,
    stopping_listener: bool,
    scanning_agents: bool,
    quitting: bool,
}

impl App {
    pub fn new(view: WebView, proxy: EventLoopProxy<Message>, start_dir: PathBuf) -> Self {
        let output = proxy.clone();
        let sessions = Sessions::new(move |event| {
            let _ = output.send_event(Message::Shell(event));
        });
        Self {
            projects: projects::start(proxy.clone()),
            opening_projects: HashSet::new(),
            view,
            proxy,
            sessions,
            companions: HashMap::new(),
            start_dir,
            contexts: HashSet::new(),
            last_dirs: HashMap::new(),
            scanning_services: false,
            stopping_listener: false,
            scanning_agents: false,
            quitting: false,
        }
    }

    fn emit(&self, name: &str, value: Value) {
        let script = format!("window.minterm?.receive({},{})", json!(name), value);
        let _ = self.view.evaluate_script(&script);
    }

    fn worker(&self, job: impl FnOnce() -> Message + Send + 'static) {
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let _ = proxy.send_event(job());
        });
    }

    pub fn quitting(&self) -> bool {
        self.quitting
    }

    pub fn act(&mut self, action: Action) {
        match action {
            Action::AgentPreferences => self.emit("preferences", ui_preferences()),
            Action::Ready => self.emit(
                "ready",
                json!({
                    "path":short_path(&self.start_dir),
                    "preferences":ui_preferences()
                }),
            ),
            Action::Open {
                id,
                from,
                cols,
                rows,
            } => self.open(id, from, cols, rows),
            Action::Projects => {
                let _ = self.projects.send(projects::Request::List);
            }
            Action::OpenProject {
                id,
                path,
                cols,
                rows,
            } => {
                if !self.last_dirs.contains_key(&id) && self.opening_projects.insert(id) {
                    let _ = self.projects.send(projects::Request::Open {
                        id,
                        path,
                        cols,
                        rows,
                    });
                }
            }
            Action::OpenAgent {
                id,
                from,
                agent,
                prompt,
                cols,
                rows,
            } => {
                let result = self
                    .sessions
                    .live_cwd(from)
                    .ok_or_else(|| "Could not resolve the current shell directory.".to_owned())
                    .and_then(|cwd| {
                        if self.companions.get(&from).is_some_and(|child| child.live)
                            || self.last_dirs.contains_key(&id)
                        {
                            return Err("This agent session is already open.".into());
                        }
                        self.sessions
                            .open_agent(id, &cwd, &agent, &prompt, cols, rows)?;
                        Ok(cwd)
                    });
                match result {
                    Ok(cwd) => {
                        if let Some(old) =
                            self.companions.insert(from, Companion { id, live: true })
                        {
                            self.last_dirs.remove(&old.id);
                        }
                        self.emit("opened", json!({"id":id,"path":short_path(&cwd)}));
                        self.last_dirs.insert(id, cwd);
                    }
                    Err(error) => self.emit("failed", json!({"id":id,"error":error})),
                }
            }
            Action::Write { id, data } => self.write(id, data.as_bytes()),
            Action::Binary { id, data } => {
                if let Ok(bytes) = STANDARD.decode(data) {
                    self.write(id, &bytes);
                }
            }
            Action::Resize { id, cols, rows } => {
                let _ = self.sessions.resize(id, cols, rows);
            }
            Action::Ack { id } => self.sessions.ack_output(id),
            Action::Close { id, force } => {
                if !force
                    && (self.companions.get(&id).is_some_and(|child| child.live)
                        || self.sessions.is_busy(id))
                {
                    self.emit("confirm", json!({"id":id}));
                } else {
                    self.opening_projects.remove(&id);
                    if let Some(child) = self.companions.remove(&id) {
                        self.sessions.close(child.id);
                        self.last_dirs.remove(&child.id);
                    }
                    self.sessions.close(id);
                    self.track_project_shells();
                    self.last_dirs.remove(&id);
                    self.emit("closed", json!({"id":id}));
                }
            }
            Action::Quit { force } => {
                let shells = self.sessions.shell_pids();
                if !force
                    && (shells.len() > 1
                        || shells.iter().any(|(id, _)| self.sessions.is_busy(*id))
                        || self.companions.values().any(|child| child.live))
                {
                    self.emit("confirm", json!({}));
                } else {
                    self.sessions.shutdown();
                    self.quitting = true;
                }
            }
            Action::Context { id } => {
                self.track_project_shells();
                if !self.contexts.contains(&id)
                    && let Some(cwd) = self
                        .sessions
                        .cwd(id)
                        .or_else(|| self.last_dirs.get(&id).cloned())
                {
                    self.contexts.insert(id);
                    self.last_dirs.insert(id, cwd.clone());
                    self.worker(move || {
                        Message::Context(
                            id,
                            json!({"id":id,"path":short_path(&cwd),"git":inspect::git(&cwd)}),
                        )
                    });
                }
            }
            Action::SaveInspector { state } => {
                if let Err(error) = state.save() {
                    self.emit("notice", json!(error));
                }
            }
            Action::SaveSplitView { enabled } => {
                if let Err(error) = crate::preferences::save_split_view(enabled) {
                    self.emit("notice", json!(error));
                }
            }
            Action::StopListener { listener } if !self.stopping_listener => {
                self.stopping_listener = true;
                self.worker(move || {
                    Message::ListenerStopped(inspect::stop_listener(&listener).err())
                });
            }
            Action::Services if !self.scanning_services => {
                self.scanning_services = true;
                self.worker(|| Message::Services(json!(inspect::services())));
            }
            Action::Agents if !self.scanning_agents => {
                self.scanning_agents = true;
                let shells = self.sessions.shell_pids();
                self.worker(move || Message::Agents(json!(agents::discover(&shells))));
            }
            Action::Drag | Action::Services | Action::StopListener { .. } | Action::Agents => {}
        }
    }

    fn directory_for(&self, from: Option<u32>) -> PathBuf {
        from.and_then(|id| {
            self.sessions
                .cwd(id)
                .or_else(|| self.last_dirs.get(&id).cloned())
        })
        .unwrap_or_else(|| self.start_dir.clone())
    }

    fn open(&mut self, id: u32, from: Option<u32>, cols: u16, rows: u16) {
        let directory = self.directory_for(from);
        self.open_directory(id, directory, cols, rows);
    }

    fn track_project_shells(&self) {
        let shells = self
            .sessions
            .shell_pids()
            .into_iter()
            .filter(|(id, _)| !self.companions.values().any(|child| child.id == *id))
            .collect();
        let _ = self.projects.send(projects::Request::Shells(shells));
    }

    fn open_directory(&mut self, id: u32, directory: PathBuf, cols: u16, rows: u16) {
        match self.sessions.open(id, &directory, cols, rows) {
            Ok(()) => {
                self.emit("opened", json!({"id": id, "path": short_path(&directory)}));
                self.last_dirs.insert(id, directory);
                self.track_project_shells();
            }
            Err(error) => self.emit("failed", json!({"id": id, "error": error})),
        }
    }

    fn write(&self, id: u32, bytes: &[u8]) {
        if let Err(error) = self.sessions.write(id, bytes) {
            self.emit("notice", json!(error));
        }
    }

    pub fn receive(&mut self, message: Message) {
        match message {
            Message::Action(action) => self.act(action),
            Message::Preferences => self.emit("preferences", ui_preferences()),
            Message::Shell(SessionEvent::Output { id, data }) => {
                self.emit("output", json!({"id":id,"data":data}))
            }
            Message::Shell(SessionEvent::Exited {
                id,
                status,
                success,
            }) => {
                if let Some(child) = self.companions.values_mut().find(|child| child.id == id) {
                    child.live = false;
                }
                self.sessions.close(id);
                self.track_project_shells();
                self.emit("exited", json!({"id":id,"status":status,"success":success}));
            }
            Message::Menu(name) => self.emit("menu", json!(name)),
            Message::Projects(value) => self.emit("projects", value),
            Message::ProjectDirectory {
                id,
                directory,
                cols,
                rows,
            } => {
                if self.opening_projects.remove(&id) {
                    match directory {
                        Ok(directory) => self.open_directory(id, directory, cols, rows),
                        Err(error) => self.emit("failed", json!({"id":id,"error":error})),
                    }
                }
            }
            Message::Context(id, value) => {
                self.contexts.remove(&id);
                self.emit("context", value);
            }
            Message::Services(value) => {
                self.scanning_services = false;
                self.emit("services", value);
            }
            Message::ListenerStopped(error) => {
                self.stopping_listener = false;
                self.emit("listenerStopped", json!(error));
            }
            Message::Agents(value) => {
                self.scanning_agents = false;
                self.emit("agents", value);
            }
        }
    }
}

fn ui_preferences() -> Value {
    let prefs = crate::preferences::read();
    let commands: Vec<_> = prefs
        .agent_commands
        .iter()
        .map(|c| json!({"name": c.name, "agent": c.agent, "shortcut": c.shortcut}))
        .collect();
    json!({
        "agent": prefs.agent,
        "agentCommands": commands,
        "agentCommandsError": prefs.agent_commands_error,
        "shortcut": prefs.shortcut,
        "splitShortcut": prefs.split_shortcut,
        "splitView": prefs.split_view,
        "newTabMode": prefs.new_tab_mode,
        "inspector": prefs.inspector
    })
}

fn short_path(path: &Path) -> String {
    if let Some(home) = env::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return Path::new("~").join(rest).display().to_string();
    }
    path.display().to_string()
}
