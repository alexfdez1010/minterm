use crate::{
    agents, assistant,
    credentials::Provider,
    inspect,
    jobs::{JobEvent, Jobs},
    sessions::{SessionEvent, Sessions},
    settings,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    thread,
};
use tao::event_loop::EventLoopProxy;
use wry::WebView;

/// Messages accepted from the embedded, local page.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    Ready,
    Open {
        id: u32,
        from: Option<u32>,
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
    StartJob {
        id: u32,
        from: Option<u32>,
        agent: String,
        prompt: String,
    },
    StopJob {
        id: u32,
    },
    Summarize {
        id: u32,
        provider: Option<Provider>,
        excerpt: String,
        revision: u32,
    },
    Services,
    Agents,
    Settings,
    Ask {
        provider: Provider,
        question: String,
    },
    Drag,
}

pub enum Message {
    Action(Action),
    Shell(SessionEvent),
    Job(JobEvent),
    Summary(u32, Value),
    Menu(String),
    Context(u32, Value),
    Services(Value),
    Agents(Value),
    Answer(Value),
}

pub struct App {
    view: WebView,
    proxy: EventLoopProxy<Message>,
    sessions: Sessions,
    jobs: Jobs,
    summaries: HashSet<u32>,
    start_dir: PathBuf,
    vault_error: Option<String>,
    contexts: HashSet<u32>,
    last_dirs: HashMap<u32, PathBuf>,
    scanning_services: bool,
    scanning_agents: bool,
    asking: bool,
    quitting: bool,
}

impl App {
    pub fn new(
        view: WebView,
        proxy: EventLoopProxy<Message>,
        start_dir: PathBuf,
        vault_error: Option<String>,
    ) -> Self {
        let output = proxy.clone();
        let sessions = Sessions::new(move |event| {
            let _ = output.send_event(Message::Shell(event));
        });
        let jobs_proxy = proxy.clone();
        let jobs = Jobs::new(move |event| {
            let _ = jobs_proxy.send_event(Message::Job(event));
        });
        Self {
            view,
            jobs,
            summaries: HashSet::new(),
            proxy,
            sessions,
            start_dir,
            vault_error,
            contexts: HashSet::new(),
            last_dirs: HashMap::new(),
            scanning_services: false,
            scanning_agents: false,
            asking: false,
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
            Action::Ready => self.emit(
                "ready",
                json!({
                    "path":short_path(&self.start_dir),
                    "vaultError":self.vault_error
                }),
            ),
            Action::Open {
                id,
                from,
                cols,
                rows,
            } => self.open(id, from, cols, rows),
            Action::Write { id, data } => self.write(id, data.as_bytes()),
            Action::Binary { id, data } => {
                if let Ok(bytes) = STANDARD.decode(data) {
                    self.write(id, &bytes);
                }
            }
            Action::Resize { id, cols, rows } => {
                let _ = self.sessions.resize(id, cols, rows);
            }
            Action::Ack { id } if self.jobs.contains(id) => self.jobs.ack_output(id),
            Action::Ack { id } => self.sessions.ack_output(id),
            Action::StartJob {
                id,
                from,
                agent,
                prompt,
            } => {
                let directory = self.directory_for(from);
                match self.jobs.start(id, &agent, &directory, &prompt) {
                    Ok(()) => {
                        self.emit("opened", json!({"id":id, "path":short_path(&directory)}));
                        self.last_dirs.insert(id, directory);
                    }
                    Err(error) => self.emit("failed", json!({"id":id,"error":error})),
                }
            }
            Action::StopJob { id } if self.jobs.running(id) => {
                self.jobs.close(id);
                self.emit(
                    "jobStatus",
                    json!({"id":id,"state":"stopped","detail":"Stopped"}),
                );
                self.emit("exited", json!({"id":id,"status":"Agent stopped."}));
            }
            Action::StopJob { .. } => {}
            Action::Summarize {
                id,
                provider,
                excerpt,
                revision,
            } => {
                if !self.last_dirs.contains_key(&id) {
                    return;
                }
                let source = match (self.jobs.provider(id), provider) {
                    (Some(provider), None) => assistant::SummarySource::Managed(provider),
                    (None, Some(provider)) => assistant::SummarySource::Reviewed(provider),
                    _ => return,
                };
                if !self.summaries.insert(id) {
                    self.emit("summary", json!({"id":id,"revision":revision,"error":"Another summary is in progress."}));
                    return;
                }
                self.worker(move || {
                    let value = match assistant::summarize(source, &excerpt) {
                        Ok(Some(text)) => json!({"id":id,"revision":revision,"text":text}),
                        Ok(None) => json!({"id":id,"revision":revision,"skipped":true}),
                        Err(error) => json!({"id":id,"revision":revision,"error":error}),
                    };
                    Message::Summary(id, value)
                });
            }
            Action::Close { id, force } => {
                if !force && (self.jobs.running(id) || self.sessions.is_busy(id)) {
                    self.emit("confirm", json!({"id":id}));
                } else {
                    self.sessions.close(id);
                    self.jobs.close(id);
                    self.last_dirs.remove(&id);
                    self.emit("closed", json!({"id":id}));
                }
            }
            Action::Quit { force } => {
                let shells = self.sessions.shell_pids();
                if !force
                    && (shells.len() > 1
                        || shells.iter().any(|(id, _)| self.sessions.is_busy(*id))
                        || self.last_dirs.keys().any(|id| self.jobs.running(*id)))
                {
                    self.emit("confirm", json!({}));
                } else {
                    self.jobs.shutdown();
                    self.sessions.shutdown();
                    self.quitting = true;
                }
            }
            Action::Context { id } => {
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
            Action::Services if !self.scanning_services => {
                self.scanning_services = true;
                self.worker(|| Message::Services(json!(inspect::services())));
            }
            Action::Agents if !self.scanning_agents => {
                self.scanning_agents = true;
                let shells = self.sessions.shell_pids();
                self.worker(move || Message::Agents(json!(agents::discover(&shells))));
            }
            Action::Ask { provider, question } if !self.asking => {
                self.asking = true;
                self.worker(move || {
                    Message::Answer(match assistant::ask(provider, &question) {
                        Ok(text) => json!({"text":text}),
                        Err(error) => json!({"error":error}),
                    })
                });
            }
            Action::Settings => settings::show(),
            Action::Drag | Action::Services | Action::Agents | Action::Ask { .. } => {}
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
        match self.sessions.open(id, &directory, cols, rows) {
            Ok(()) => {
                self.emit("opened", json!({"id": id, "path": short_path(&directory)}));
                self.last_dirs.insert(id, directory);
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
            Message::Summary(id, value) => {
                self.summaries.remove(&id);
                if self.last_dirs.contains_key(&id) {
                    self.emit("summary", value);
                }
            }
            Message::Job(JobEvent::Output { id, data }) => {
                if self.jobs.contains(id) {
                    self.emit("output", json!({"id":id,"data":data}));
                }
            }
            Message::Job(JobEvent::Status { id, state, detail }) => {
                if self.jobs.contains(id) {
                    self.emit("jobStatus", json!({"id":id,"state":state,"detail":detail}));
                }
            }
            Message::Job(JobEvent::Exited { id, status }) => {
                if self.jobs.contains(id) {
                    self.emit("exited", json!({"id":id,"status":status}));
                }
            }
            Message::Shell(SessionEvent::Output { id, data }) => {
                self.emit("output", json!({"id":id,"data":data}))
            }
            Message::Shell(SessionEvent::Exited { id, status }) => {
                self.sessions.close(id);
                self.emit("exited", json!({"id":id,"status":status}));
            }
            Message::Menu(name) => self.emit("menu", json!(name)),
            Message::Context(id, value) => {
                self.contexts.remove(&id);
                self.emit("context", value);
            }
            Message::Services(value) => {
                self.scanning_services = false;
                self.emit("services", value);
            }
            Message::Agents(value) => {
                self.scanning_agents = false;
                self.emit("agents", value);
            }
            Message::Answer(value) => {
                self.asking = false;
                self.emit("answer", value);
            }
        }
    }
}

fn short_path(path: &Path) -> String {
    if let Some(home) = env::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return Path::new("~").join(rest).display().to_string();
    }
    path.display().to_string()
}
