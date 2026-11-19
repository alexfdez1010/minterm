use crate::prompt::ShellIntegration;
use base64::{Engine, engine::general_purpose::STANDARD};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    collections::{HashMap, VecDeque},
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    mem::{MaybeUninit, size_of},
    os::{
        fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt, net::UnixStream},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{Arc, Mutex, Weak},
    thread::{self, JoinHandle},
    time::Duration,
};

const BLOCK: usize = 64 * 1024;
const OUTPUT_WINDOW: usize = 4;
const INPUT_LIMIT: usize = 4 * 1024 * 1024;
type Notify = Arc<dyn Fn(SessionEvent) + Send + Sync>;

#[derive(Debug)]
pub enum SessionEvent {
    Output {
        id: u32,
        data: String,
    },
    Exited {
        id: u32,
        status: String,
        success: bool,
    },
}

pub struct Sessions {
    prompt: ShellIntegration,
    tabs: HashMap<u32, Session>,
    retiring: Vec<JoinHandle<()>>,
    notify: Notify,
}

type Master = Mutex<Box<dyn MasterPty + Send>>;

struct Session {
    master: Weak<Master>,
    shell: i32,
    start: PathBuf,
    queue: Arc<Mutex<Queue>>,
    wake: UnixStream,
    worker: JoinHandle<()>,
}

impl Session {
    fn foreground_group(&self) -> Option<i32> {
        self.master
            .upgrade()?
            .lock()
            .unwrap()
            .process_group_leader()
    }
}

#[derive(Default)]
struct Queue {
    input: VecDeque<u8>,
    in_flight: usize,
    closed: bool,
}

impl Sessions {
    pub fn new(notify: impl Fn(SessionEvent) + Send + Sync + 'static) -> Self {
        Self {
            prompt: ShellIntegration::default(),
            tabs: HashMap::new(),
            retiring: Vec::new(),
            notify: Arc::new(notify),
        }
    }

    pub fn open(&mut self, id: u32, cwd: &Path, cols: u16, rows: u16) -> Result<(), String> {
        if self.tabs.contains_key(&id) {
            return Err("This session is already open".into());
        }
        let mut cmd = CommandBuilder::new_default_prog();
        if let Err(error) = self.prompt.configure(&mut cmd) {
            eprintln!("Could not install the Minterm prompt: {error}");
        }
        cmd.cwd(cwd);
        // Make the bundled CLI available to login shells started from Finder.
        if let Ok(exe) = std::env::current_exe()
            && let Some(bin) = exe.parent()
            && let Some(path) = cmd.get_env("PATH")
        {
            let entries = std::iter::once(bin.to_owned()).chain(std::env::split_paths(path));
            if let Ok(path) = std::env::join_paths(entries) {
                cmd.env("PATH", path);
            }
        }
        terminal_environment(&mut cmd);
        self.spawn(id, cwd, size(cols, rows), cmd)
    }

    pub fn open_agent(
        &mut self,
        id: u32,
        cwd: &Path,
        agent: &str,
        prompt: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), String> {
        if self.tabs.contains_key(&id) {
            return Err("This session is already open".into());
        }
        if prompt.len() > 128 * 1024 || prompt.contains('\0') {
            return Err("Enter a task of up to 128 KiB without NUL characters.".into());
        }
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err("The active working directory is unavailable.".into());
        }
        let prefs = crate::preferences::read();
        if let Some(error) = prefs.agent_commands_error {
            return Err(error);
        }
        let cmd = if let Some(profile) = prefs.agent_command(agent) {
            let shell = CommandBuilder::new_default_prog().get_shell();
            let cli =
                crate::agent_cli::AgentCli::shell_command(Path::new(&shell), &profile.command, cwd);
            interactive_agent(&profile.agent, cli, cwd, prompt, None)
        } else {
            let cli = crate::agent_cli::resolve(agent, prefs.executable(agent))?;
            let ollama = (agent == "codex" && prefs.codex_ollama)
                .then_some((prefs.ollama_host.as_str(), prefs.ollama_model.as_str()));
            interactive_agent(agent, cli, cwd, prompt, ollama)
        };
        self.spawn(id, cwd, size(cols, rows), cmd)
    }

    fn spawn(
        &mut self,
        id: u32,
        cwd: &Path,
        size: PtySize,
        cmd: CommandBuilder,
    ) -> Result<(), String> {
        let pair = native_pty_system().openpty(size).map_err(message)?;
        let fd = pair
            .master
            .as_raw_fd()
            .ok_or("Missing terminal descriptor")?;
        // The master owns fd and stays alive until after this duplicate is created.
        let terminal = File::from(
            unsafe { BorrowedFd::borrow_raw(fd) }
                .try_clone_to_owned()
                .map_err(message)?,
        );
        nonblocking(terminal.as_raw_fd()).map_err(message)?;
        let (wake, mut signal) = UnixStream::pair().map_err(message)?;
        wake.set_nonblocking(true).map_err(message)?;
        signal.set_nonblocking(true).map_err(message)?;
        let device = pair
            .master
            .tty_name()
            .and_then(|p| p.metadata().ok())
            .map(|m| m.rdev() as u32);
        let events = EventQueue::new().map_err(message)?;
        let child = pair.slave.spawn_command(cmd).map_err(message)?;
        drop(pair.slave);
        let master = Arc::new(Mutex::new(pair.master));
        let resize_handle = Arc::downgrade(&master);
        let mut shell = Shell {
            pid: child.process_id().expect("Unix shell PID") as i32,
            child,
            device,
            pty: Some((terminal, master)),
            reaped: false,
        };
        let pid = shell.pid;
        let queue = Arc::new(Mutex::new(Queue::default()));
        let state = Arc::clone(&queue);
        let notify = Arc::clone(&self.notify);
        let worker = thread::Builder::new()
            .name(format!("minterm-pty-{id}"))
            .spawn(move || {
                let terminal = &mut shell.pty.as_mut().unwrap().0;
                let result = run_io(&events, terminal, &mut signal, pid, &state, |data| {
                    notify(SessionEvent::Output {
                        id,
                        data: STANDARD.encode(data),
                    });
                });
                let explicitly_closed = {
                    let mut queue = state.lock().unwrap();
                    let was_closed = queue.closed;
                    queue.closed = true;
                    queue.input.clear();
                    was_closed
                };
                let (status, success) = match result {
                    Ok(true) => shell
                        .wait()
                        .map(|s| (s.to_string(), s.success()))
                        .unwrap_or_else(|error| (error, false)),
                    _ => {
                        shell.terminate();
                        (
                            result
                                .err()
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "Session closed".to_owned()),
                            false,
                        )
                    }
                };
                if !explicitly_closed {
                    notify(SessionEvent::Exited {
                        id,
                        status,
                        success,
                    });
                }
            })
            .map_err(message)?;
        self.tabs.insert(
            id,
            Session {
                master: resize_handle,
                shell: pid,
                start: cwd.into(),
                queue,
                wake,
                worker,
            },
        );
        Ok(())
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        let session = self.tabs.get(&id).ok_or("Session closed")?;
        let mut queue = session.queue.lock().unwrap();
        if queue.closed {
            return Err("Session closed".into());
        }
        if data.len() > INPUT_LIMIT - queue.input.len() {
            return Err("Input exceeds the 4 MiB queue. Paste a smaller selection.".into());
        }
        let needs_wakeup = queue.input.is_empty();
        queue.input.extend(data);
        drop(queue);
        if needs_wakeup {
            wake(&session.wake);
        }
        Ok(())
    }

    pub fn ack_output(&self, id: u32) {
        if let Some(session) = self.tabs.get(&id) {
            let mut queue = session.queue.lock().unwrap();
            let was_full = queue.in_flight == OUTPUT_WINDOW;
            queue.in_flight = queue.in_flight.saturating_sub(1);
            drop(queue);
            if was_full {
                wake(&session.wake);
            }
        }
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let master = self
            .tabs
            .get(&id)
            .and_then(|session| session.master.upgrade())
            .ok_or("Session closed")?;
        master
            .lock()
            .unwrap()
            .resize(size(cols, rows))
            .map_err(message)
    }

    pub fn shell_pids(&self) -> Vec<(u32, i32)> {
        self.tabs
            .iter()
            .filter(|(_, session)| !session.queue.lock().unwrap().closed)
            .map(|(&id, session)| (id, session.shell))
            .collect()
    }

    pub fn is_busy(&self, id: u32) -> bool {
        self.tabs.get(&id).is_some_and(|session| {
            !session.queue.lock().unwrap().closed
                && (session
                    .foreground_group()
                    .is_some_and(|pid| pid != session.shell)
                    || crate::agents::has_jobs(session.shell))
        })
    }

    pub fn cwd(&self, id: u32) -> Option<PathBuf> {
        self.read_cwd(id, true)
    }

    pub fn live_cwd(&self, id: u32) -> Option<PathBuf> {
        self.read_cwd(id, false)
    }

    fn read_cwd(&self, id: u32, allow_initial: bool) -> Option<PathBuf> {
        let session = self.tabs.get(&id)?;
        let queue = session.queue.lock().unwrap();
        if queue.closed {
            return None;
        }
        session
            .foreground_group()
            .and_then(process_cwd)
            .or_else(|| process_cwd(session.shell))
            .or_else(|| allow_initial.then(|| session.start.clone()))
    }

    pub fn close(&mut self, id: u32) {
        if let Some(session) = self.tabs.remove(&id) {
            session.queue.lock().unwrap().closed = true;
            wake(&session.wake);
            self.retiring.retain(|thread| !thread.is_finished());
            self.retiring.push(session.worker);
        }
    }
    pub fn shutdown(&mut self) {
        for id in self.tabs.keys().copied().collect::<Vec<_>>() {
            self.close(id);
        }
        for worker in self.retiring.drain(..) {
            let _ = worker.join();
        }
        self.prompt = ShellIntegration::default();
    }
}

impl Drop for Sessions {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn interactive_agent(
    name: &str,
    cli: crate::agent_cli::AgentCli,
    cwd: &Path,
    prompt: &str,
    ollama: Option<(&str, &str)>,
) -> CommandBuilder {
    // portable-pty falls back to HOME if the cwd disappears. The final chdir
    // in env either succeeds in that exact directory or aborts before exec.
    let mut cmd = CommandBuilder::new("/usr/bin/env");
    cmd.arg("-C");
    cmd.arg(cwd);
    cmd.arg(cli.program);
    cmd.args(cli.args);
    if let Some((host, model)) = ollama {
        cmd.args(["--oss", "--local-provider", "ollama"]);
        if !model.trim().is_empty() {
            cmd.args(["-m", model]);
        }
        let base = host.trim_end_matches('/');
        cmd.env(
            "CODEX_OSS_BASE_URL",
            if base.ends_with("/v1") {
                base.to_owned()
            } else {
                format!("{base}/v1")
            },
        );
    }
    if !prompt.is_empty() {
        if name == "opencode" {
            // The CLI parses a separate "--help" value as a flag. Bind the
            // option to its complete value so every initial prompt stays literal.
            cmd.arg(format!("--prompt={prompt}"));
        } else {
            cmd.args(["--", prompt]);
        }
    }
    cmd.cwd(cwd);
    if let Some(path) = cli.path {
        cmd.env("PATH", path);
    }
    terminal_environment(&mut cmd);
    cmd
}

fn terminal_environment(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Minterm");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    if std::env::var_os("LANG").is_none() {
        cmd.env("LANG", "en_US.UTF-8");
    }
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.max(1),
        rows: rows.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn process_cwd(pid: i32) -> Option<PathBuf> {
    let mut info = MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let len = size_of::<libc::proc_vnodepathinfo>() as i32;
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            len,
        )
    };
    if written != len {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let path = info.pvi_cdir.vip_path.as_flattened();
    let len = path.iter().position(|&c| c == 0)?;
    let bytes = unsafe { std::slice::from_raw_parts(path.as_ptr().cast::<u8>(), len) };
    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(bytes)))
}

fn wake(mut stream: &UnixStream) {
    // A full socket already contains a wakeup.
    loop {
        match stream.write(&[1]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            _ => return,
        }
    }
}

fn nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct EventQueue(OwnedFd);

impl EventQueue {
    fn new() -> io::Result<Self> {
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(fd))
    }

    fn poll(&self, changes: &[libc::kevent], ready: &mut [libc::kevent]) -> io::Result<usize> {
        loop {
            let count = unsafe {
                libc::kevent(
                    self.0.as_raw_fd(),
                    changes.as_ptr(),
                    changes.len() as i32,
                    ready.as_mut_ptr(),
                    ready.len() as i32,
                    ptr::null(),
                )
            };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

fn event(ident: i32, filter: i16, flags: u16) -> libc::kevent {
    libc::kevent {
        ident: ident as usize,
        filter,
        flags,
        fflags: 0,
        data: 0,
        udata: ptr::null_mut(),
    }
}

// One sleeping worker per tab. The renderer grants credits after parsing each
// block, so a fast producer eventually blocks in the PTY instead of growing RAM.
fn run_io(
    events: &EventQueue,
    terminal: &mut File,
    signal: &mut UnixStream,
    pid: i32,
    queue: &Mutex<Queue>,
    mut output: impl FnMut(&[u8]),
) -> io::Result<bool> {
    let fd = terminal.as_raw_fd();
    let mut process = event(pid, libc::EVFILT_PROC, libc::EV_ADD | libc::EV_ONESHOT);
    process.fflags = libc::NOTE_EXIT;
    events.poll(
        &[event(signal.as_raw_fd(), libc::EVFILT_READ, libc::EV_ADD)],
        &mut [],
    )?;
    let mut exited = match events.poll(&[process], &mut []) {
        Ok(_) => false,
        Err(e) if e.raw_os_error() == Some(libc::ESRCH) => true,
        Err(e) => return Err(e),
    };
    let mut buffer = vec![0; BLOCK];
    let mut eof = false;
    loop {
        let state = queue.lock().unwrap();
        if state.closed {
            return Ok(false);
        }
        if exited && eof {
            return Ok(true);
        }
        let can_read = !eof && state.in_flight < OUTPUT_WINDOW;
        let mut wants_write = !state.input.is_empty() && !exited;
        drop(state);
        if wants_write {
            let mut state = queue.lock().unwrap();
            let input = state.input.as_slices().0;
            match terminal.write(&input[..input.len().min(BLOCK)]) {
                Ok(n) if n > 0 => {
                    state.input.drain(..n);
                    if state.input.is_empty() && state.input.capacity() > BLOCK {
                        state.input.shrink_to_fit();
                    }
                }
                Ok(_) => return Err(io::ErrorKind::WriteZero.into()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                Err(e) => return Err(e),
            }
            wants_write = !state.input.is_empty();
        }
        if can_read {
            let mut used = 0;
            while used < buffer.len() {
                match terminal.read(&mut buffer[used..]) {
                    Ok(0) => {
                        eof = true;
                        break;
                    }
                    Ok(n) => used += n,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                        eof = true;
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
            if used > 0 {
                queue.lock().unwrap().in_flight += 1;
                output(&buffer[..used]);
            }
            if exited && used < buffer.len() {
                return Ok(true);
            }
            if used > 0 {
                continue;
            }
        }
        let filter = |enabled| {
            libc::EV_ADD
                | if enabled {
                    libc::EV_ENABLE
                } else {
                    libc::EV_DISABLE
                }
        };
        let changes = [
            event(fd, libc::EVFILT_READ, filter(can_read && !eof)),
            event(fd, libc::EVFILT_WRITE, filter(wants_write)),
        ];
        let mut ready = [process; 4];
        let count = events.poll(&changes, &mut ready)?;
        handle_events(&ready[..count], signal, &mut exited)?;
    }
}

fn handle_events(
    ready: &[libc::kevent],
    signal: &mut UnixStream,
    exited: &mut bool,
) -> io::Result<()> {
    for item in ready {
        if item.flags & libc::EV_ERROR != 0 && item.data != 0 {
            return Err(io::Error::from_raw_os_error(item.data as i32));
        }
        if item.filter == libc::EVFILT_PROC {
            *exited = true;
        }
        if item.ident == signal.as_raw_fd() as usize && item.filter == libc::EVFILT_READ {
            let mut buf = [0; 1024];
            while let Ok(n) = signal.read(&mut buf) {
                if n < buf.len() {
                    break;
                }
            }
        }
    }
    Ok(())
}

struct Shell {
    pid: i32,
    child: Box<dyn Child + Send + Sync>,
    device: Option<u32>,
    // The worker owns both descriptors. UI calls borrow the master briefly for
    // ioctl operations and cannot hold the terminal open throughout shutdown.
    pty: Option<(File, Arc<Master>)>,
    reaped: bool,
}

impl Shell {
    fn wait(&mut self) -> Result<portable_pty::ExitStatus, String> {
        let status = self.child.wait().map_err(message)?;
        self.reaped = true;
        Ok(status)
    }

    fn terminate(&mut self) {
        if self.reaped {
            return;
        }
        // Keep the session leader unreaped until our last signal. Its PID cannot
        // be reused as another session ID while we inspect the attached jobs.
        let mut pids = self.attached_jobs();
        self.signal(&pids, libc::SIGHUP);
        thread::sleep(Duration::from_millis(150));
        pids.extend(self.attached_jobs());
        self.signal(&pids, libc::SIGKILL);
        // Darwin may keep an exiting TUI in ttywait while the slave drains.
        // Closing both master descriptors lets it complete exit before wait4.
        drop(self.pty.take());
        let _ = self.wait();
    }

    fn attached_jobs(&self) -> Vec<i32> {
        self.device.map(crate::agents::tty_pids).unwrap_or_default()
    }

    fn signal(&self, pids: &[i32], signal: i32) {
        for &pid in pids {
            if pid > 0 && pid != self.pid && unsafe { libc::getsid(pid) } == self.pid {
                unsafe {
                    libc::kill(pid, signal);
                }
            }
        }
        // This is our direct, unreaped child. The kernel reserves its PID even
        // when its session is gone and getsid(pid) already returns ESRCH.
        unsafe { libc::kill(self.pid, signal) };
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc::{self, Receiver},
        time::Instant,
    };

    const ID: u32 = 7;

    fn command(script: &str, interactive: bool) -> (Sessions, Receiver<SessionEvent>) {
        let (tx, rx) = mpsc::channel();
        let mut sessions = Sessions::new(move |event| {
            let _ = tx.send(event);
        });
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.arg(if interactive { "-ic" } else { "-c" });
        cmd.arg(script);
        cmd.env("ENV", "/dev/null");
        cmd.env("PS1", "");
        cmd.cwd("/");
        sessions
            .spawn(ID, Path::new("/"), size(80, 24), cmd)
            .unwrap();
        (sessions, rx)
    }

    fn next(rx: &Receiver<SessionEvent>) -> SessionEvent {
        rx.recv_timeout(Duration::from_secs(6)).expect("PTY event")
    }

    fn read_until(sessions: &Sessions, rx: &Receiver<SessionEvent>, marker: &str) -> String {
        let mut received = Vec::new();
        loop {
            match next(rx) {
                SessionEvent::Output { id: ID, data } => {
                    received.extend(STANDARD.decode(data).unwrap());
                    sessions.ack_output(ID);
                    if received
                        .windows(marker.len())
                        .any(|part| part == marker.as_bytes())
                    {
                        return String::from_utf8(received).unwrap();
                    }
                }
                other => panic!("Expected {marker:?} before {other:?}"),
            }
        }
    }

    #[test]
    fn login_prompts_keep_profile_and_treat_git_names_as_data() {
        let branch = "topic/$(touch${IFS}BRANCH_INJECTION)%F{blue}";
        let directory = "work$(touch${IFS}DIR_INJECTION)%n";
        for shell in ["/bin/bash", "/bin/zsh"] {
            let fixture = tempfile::tempdir().unwrap();
            let home = fixture.path().join("home");
            let work = fixture.path().join(directory);
            std::fs::create_dir(&home).unwrap();
            std::fs::create_dir_all(work.join(".git")).unwrap();
            let profile = if shell.ends_with("bash") {
                ".bash_profile"
            } else {
                ".zshrc"
            };
            std::fs::write(
                home.join(profile),
                "alias testprofile='printf PROFILE_KEPT'\n",
            )
            .unwrap();
            let head = work.join(".git/HEAD");
            std::fs::write(&head, format!("ref: refs/heads/{branch}\n")).unwrap();

            let (tx, rx) = mpsc::channel();
            let mut sessions = Sessions::new(move |event| {
                let _ = tx.send(event);
            });
            let mut cmd = CommandBuilder::new_default_prog();
            cmd.env_clear();
            cmd.env("HOME", &home);
            cmd.env("SHELL", shell);
            cmd.env("LANG", "en_US.UTF-8");
            cmd.env("TERM", "xterm-256color");
            cmd.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
            cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
            cmd.cwd(&work);
            sessions.prompt.configure(&mut cmd).unwrap();
            sessions.spawn(ID, &work, size(180, 24), cmd).unwrap();
            let initial = read_until(&sessions, &rx, "❯");
            assert!(
                initial.contains(directory) && initial.contains(branch),
                "{shell}: {initial:?}"
            );

            std::fs::write(&head, "1234567890abcdef1234567890abcdef12345678\n").unwrap();
            sessions.write(ID, b"testprofile; false\r").unwrap();
            let updated = read_until(&sessions, &rx, "❯");
            assert!(
                updated.contains("PROFILE_KEPT") && updated.contains("1234567"),
                "{shell}: {updated:?}"
            );
            assert!(
                updated.contains("33m"),
                "Missing failure color from {shell}: {updated:?}"
            );
            assert!(!work.join("BRANCH_INJECTION").exists());
            assert!(!work.join("DIR_INJECTION").exists());
            if shell.ends_with("bash") {
                std::fs::write(work.join("minterm-completion-fixture"), "").unwrap();
                sessions
                    .write(ID, b"printf '<%s>\\n' minterm-complet\x1b[Z\r")
                    .unwrap();
                let completion = read_until(&sessions, &rx, "❯");
                assert!(
                    completion.contains("\r\n<minterm-completion-fixture>\r\n"),
                    "{completion:?}"
                );
            }
        }
    }

    #[test]
    fn shell_suggestions_accept_only_on_explicit_key() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path();
        std::fs::write(
            home.join(".zshrc"),
            r#"
            bindkey -e
            _snap() {
              local -a mine=("${(@M)region_highlight:#* memo=minterm-suggestion}")
              print -rl -- "$BUFFER" "$POSTDISPLAY" "${#mine}" > "$HOME/snapshot"
            }
            zle -N _snap
            bindkey '^X?' _snap
            alias mintfixturecommand='print -r -- EXPLICITLY_ACCEPTED'
        "#,
        )
        .unwrap();
        let (tx, rx) = mpsc::channel();
        let mut sessions = Sessions::new(move |event| {
            let _ = tx.send(event);
        });
        let mut cmd = CommandBuilder::new_default_prog();
        cmd.env_clear();
        for (key, value) in [
            ("SHELL", "/bin/zsh"),
            ("TERM", "xterm-256color"),
            ("LANG", "en_US.UTF-8"),
            ("PATH", "/bin:/usr/bin"),
        ] {
            cmd.env(key, value);
        }
        cmd.env("HOME", home);
        cmd.cwd(home);
        sessions.prompt.configure(&mut cmd).unwrap();
        sessions.spawn(ID, home, size(140, 24), cmd).unwrap();
        read_until(&sessions, &rx, "❯");
        let snapshot = |expected: &str| {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                sessions.write(ID, b"\x18?").unwrap();
                thread::sleep(Duration::from_millis(15));
                // The hidden test does not have a WebView to acknowledge repaint output.
                for _ in rx.try_iter() {
                    sessions.ack_output(ID);
                }
                let value = std::fs::read_to_string(home.join("snapshot")).unwrap_or_default();
                if value == expected {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Expected {expected:?}, received {value:?}"
                );
            }
        };
        sessions.write(ID, b"mintfix").unwrap();
        snapshot("mintfix\nturecommand\n1\n");
        sessions.write(ID, b"\x1b[C").unwrap();
        snapshot("mintfixturecommand\n\n0\n");
        sessions.write(ID, b"\r").unwrap();
        read_until(&sessions, &rx, "EXPLICITLY_ACCEPTED");
        sessions
            .write(ID, b"print -r -- history-fixture\r")
            .unwrap();
        read_until(&sessions, &rx, "❯");
        sessions.write(ID, b"print -r -- hist").unwrap();
        snapshot("print -r -- hist\nory-fixture\n1\n");
        sessions.write(ID, b"\x1b[D").unwrap();
        snapshot("print -r -- hist\n\n0\n");
        sessions.write(ID, b"\x1b[C").unwrap();
        snapshot("print -r -- hist\nory-fixture\n1\n");
        sessions.write(ID, b"\r").unwrap();
        let output = read_until(&sessions, &rx, "❯");
        assert!(
            output.contains("\r\nhist\r\n"),
            "Enter accepted the preview: {output:?}"
        );
    }

    #[test]
    fn interactive_agent_gets_a_tty_literal_argument_and_the_selected_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let work = fixture.path().join("working directory");
        std::fs::create_dir(&work).unwrap();
        let program = fixture.path().join("custom agent");
        std::fs::write(
            &program,
            r#"#!/bin/sh
            test -t 0 && test -t 1 && test -t 2 || exit 41
            printf '%s\0' "$@" > args
            printf '%s' "$CODEX_OSS_BASE_URL" > endpoint
            printf 'ready\n'
            read -r line
            printf '<%s>\n' "$line"
        "#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let prompt = "--leading option $(touch surprise)\nand another line";
        for (name, oss) in [
            ("codex", None),
            ("codex", Some(("http://localhost:1234", "chosen model"))),
            ("claude", None),
            ("opencode", None),
        ] {
            let (tx, rx) = mpsc::channel();
            let mut sessions = Sessions::new(move |event| {
                let _ = tx.send(event);
            });
            let cli = crate::agent_cli::AgentCli {
                program: program.clone(),
                args: Vec::new(),
                path: None,
            };
            let cmd = interactive_agent(name, cli, &work, prompt, oss);
            sessions.spawn(ID, &work, size(100, 20), cmd).unwrap();
            read_until(&sessions, &rx, "ready");
            assert_eq!(sessions.live_cwd(ID).unwrap(), work.canonicalize().unwrap());
            let args = std::fs::read(work.join("args")).unwrap();
            let mut expected = Vec::new();
            if oss.is_some() {
                expected.extend_from_slice(b"--oss\0--local-provider\0ollama\0-m\0chosen model\0");
                assert_eq!(
                    std::fs::read_to_string(work.join("endpoint")).unwrap(),
                    "http://localhost:1234/v1"
                );
            }
            expected.extend_from_slice(if name == "opencode" {
                b"--prompt="
            } else {
                b"--\0"
            });
            expected.extend_from_slice(prompt.as_bytes());
            expected.push(0);
            assert_eq!(args, expected);
            assert!(!work.join("surprise").exists());
            sessions.write(ID, b"interactive input\r").unwrap();
            read_until(&sessions, &rx, "<interactive input>");
        }
    }

    #[test]
    fn configured_commands_expand_login_aliases_and_functions_in_the_tab_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path();
        let work = home.join("project 'with spaces' $(touch wrong)");
        std::fs::create_dir(&work).unwrap();
        let startup = r#"
            cd /
            probe() {
                test -t 0 && test -t 1 && test -t 2 || return 41
                for arg do printf '%s\0' "$arg"; done > args
                printf 'custom-ready\n'
                IFS= read -r line
                printf '<%s>\n' "$line"
            }
            alias ocq='probe -m "qwen local"'
        "#;
        let prompt = "--help \" ' $(touch surprise)\n`touch surprise`";
        for (shell, startup_file) in [("/bin/zsh", ".zshrc"), ("/bin/bash", ".bash_profile")] {
            std::fs::write(home.join(startup_file), startup).unwrap();
            for (name, command, task, expected) in [
                (
                    "opencode",
                    "ocq",
                    prompt,
                    vec![
                        "-m".into(),
                        "qwen local".into(),
                        format!("--prompt={prompt}"),
                    ],
                ),
                (
                    "claude",
                    "probe --model 'my model'",
                    prompt,
                    vec![
                        "--model".into(),
                        "my model".into(),
                        "--".into(),
                        prompt.into(),
                    ],
                ),
                (
                    "opencode",
                    "ocq",
                    "",
                    vec!["-m".into(), "qwen local".into()],
                ),
            ] {
                let cli =
                    crate::agent_cli::AgentCli::shell_command(Path::new(shell), command, &work);
                let mut cmd = interactive_agent(name, cli, &work, task, None);
                cmd.env_clear();
                cmd.env("HOME", home);
                cmd.env("ZDOTDIR", home);
                cmd.env("PATH", "/usr/bin:/bin");
                cmd.env("BASH_SILENCE_DEPRECATION_WARNING", "1");
                let (tx, rx) = mpsc::channel();
                let mut sessions = Sessions::new(move |event| {
                    let _ = tx.send(event);
                });
                sessions.spawn(ID, &work, size(100, 20), cmd).unwrap();
                read_until(&sessions, &rx, "custom-ready");
                assert_eq!(
                    std::fs::read(work.join("args")).unwrap(),
                    (expected.join("\0") + "\0").as_bytes()
                );
                assert!(!work.join("surprise").exists());
                assert_eq!(sessions.live_cwd(ID).unwrap(), work.canonicalize().unwrap());
                sessions.write(ID, b"next message\r").unwrap();
                read_until(&sessions, &rx, "<next message>");
            }
        }
    }

    #[test]
    fn interactive_io_resize_and_current_directory() {
        let (sessions, rx) = command(
            "stty -echo; cd /private/tmp; echo ready; read -r word; stty size; printf '[%s]' \"$word\"; exit 7",
            false,
        );
        read_until(&sessions, &rx, "ready");
        assert_eq!(sessions.cwd(ID).unwrap(), Path::new("/private/tmp"));
        sessions.resize(ID, 106, 32).unwrap();
        sessions.write(ID, b"hello\r").unwrap();
        let output = read_until(&sessions, &rx, "[hello]");
        assert!(output.contains("32 106"), "{output:?}");
        assert!(
            matches!(next(&rx), SessionEvent::Exited { id: ID, status, .. } if status.contains('7'))
        );
    }

    #[test]
    fn output_waits_for_renderer_credit_without_losing_bytes() {
        let total = 512 * 1024;
        let (sessions, rx) = command("/usr/bin/head -c 524288 /dev/zero", false);
        let mut read = 0;
        for _ in 0..OUTPUT_WINDOW {
            let SessionEvent::Output { data, .. } = next(&rx) else {
                panic!("Early exit")
            };
            read += STANDARD.decode(data).unwrap().len();
        }
        assert!(rx.recv_timeout(Duration::from_millis(80)).is_err());
        for _ in 0..OUTPUT_WINDOW {
            sessions.ack_output(ID);
        }
        loop {
            match next(&rx) {
                SessionEvent::Output { data, .. } => {
                    let bytes = STANDARD.decode(data).unwrap();
                    assert!(bytes.iter().all(|&b| b == 0));
                    read += bytes.len();
                    sessions.ack_output(ID);
                }
                SessionEvent::Exited { status, .. } => {
                    assert_eq!(status, "Success");
                    break;
                }
            }
        }
        assert_eq!(read, total);
    }

    #[test]
    fn busy_guard_covers_background_jobs_and_agent_exec() {
        let (sessions, events) = command(
            "echo ready; read -r line; sleep 10 & echo started; read -r line",
            true,
        );
        read_until(&sessions, &events, "ready");
        assert!(!sessions.is_busy(ID));
        sessions.write(ID, b"start\r").unwrap();
        read_until(&sessions, &events, "started");
        assert_eq!(
            sessions.tabs[&ID].foreground_group(),
            Some(sessions.tabs[&ID].shell)
        );
        assert!(sessions.is_busy(ID));
        drop(sessions);

        let temp = tempfile::tempdir().unwrap();
        let program = crate::agents::compile_agent_fixture(temp.path());
        let marker = temp.path().join("exec-ready");
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", r#"exec "$1" "$2""#, "sh"]);
        cmd.arg(program);
        cmd.arg(&marker);
        let mut sessions = Sessions::new(|_| {});
        sessions
            .spawn(ID, Path::new("/"), size(80, 24), cmd)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.is_file() {
            assert!(Instant::now() < deadline, "Exec agent failed to start");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            sessions.tabs[&ID].foreground_group(),
            Some(sessions.tabs[&ID].shell)
        );
        assert!(sessions.is_busy(ID), "Exec agent went undetected");
    }

    #[test]
    fn closing_a_flooded_raw_terminal_reaps_without_more_renderer_credit() {
        let (mut sessions, output) = command(
            "stty raw -echo; trap '' HUP; exec /usr/bin/yes pending",
            false,
        );
        for _ in 0..OUTPUT_WINDOW {
            assert!(matches!(next(&output), SessionEvent::Output { .. }));
        }
        // The UI is closing and will never acknowledge those blocks. The slave
        // may still be waiting for the master to consume its queued output.
        let pid = sessions.tabs[&ID].shell;
        sessions.close(ID);
        let worker = sessions.retiring.pop().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !worker.is_finished() {
            assert!(
                Instant::now() < deadline,
                "Worker stuck waiting for raw PTY exit"
            );
            thread::sleep(Duration::from_millis(10));
        }
        worker.join().unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn direct_child_signal_does_not_require_a_session_match_or_hit_foreign_pids() {
        use std::process::Command;
        let child = Command::new("/bin/sleep").arg("2").spawn().unwrap();
        let mut unrelated = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let mut shell = Shell {
            pid: child.id() as i32,
            child: Box::new(child),
            device: None,
            pty: None,
            reaped: false,
        };
        assert_ne!(unsafe { libc::getsid(shell.pid) }, shell.pid);
        shell.signal(&[unrelated.id() as i32], libc::SIGHUP);
        shell.terminate();
        let owned_was_signalled = !shell.child.wait().unwrap().success();
        let unrelated_survived = unrelated.try_wait().unwrap().is_none();
        let _ = unrelated.kill();
        unrelated.wait().unwrap();
        assert!(owned_was_signalled && unrelated_survived);
    }

    #[test]
    fn closing_stops_foreground_and_background_terminal_jobs() {
        let (mut sessions, rx) = command("trap '' HUP; sleep 60 & echo bg=$! done; sleep 60", true);
        let text = read_until(&sessions, &rx, " done");
        let background: i32 = text
            .split("bg=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while sessions.tabs[&ID]
            .foreground_group()
            .is_none_or(|pid| pid == sessions.tabs[&ID].shell)
        {
            assert!(Instant::now() < deadline, "No foreground job");
            thread::sleep(Duration::from_millis(5));
        }
        let foreground = sessions.tabs[&ID].foreground_group().unwrap();
        assert_ne!(background, foreground);
        sessions.close(ID);
        drop(sessions);
        let deadline = Instant::now() + Duration::from_secs(2);
        for pid in [foreground, background] {
            while unsafe { libc::kill(pid, 0) } == 0 {
                assert!(
                    Instant::now() < deadline,
                    "Job {pid} survived closing its terminal"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
