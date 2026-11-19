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
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

const BLOCK: usize = 64 * 1024;
const OUTPUT_WINDOW: usize = 4;
const INPUT_LIMIT: usize = 4 * 1024 * 1024;
type Notify = Arc<dyn Fn(SessionEvent) + Send + Sync>;

#[derive(Debug)]
pub enum SessionEvent {
    Output { id: u32, data: String },
    Exited { id: u32, status: String },
}

pub struct Sessions {
    prompt: ShellIntegration,
    tabs: HashMap<u32, Session>,
    retiring: Vec<JoinHandle<()>>,
    notify: Notify,
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    shell: i32,
    start: PathBuf,
    queue: Arc<Mutex<Queue>>,
    wake: UnixStream,
    worker: JoinHandle<()>,
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
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Minterm");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        if std::env::var_os("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
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
        let mut terminal = File::from(
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
        let mut shell = Shell {
            pid: child.process_id().expect("Unix shell PID") as i32,
            child,
            device,
            reaped: false,
        };
        let pid = shell.pid;
        let queue = Arc::new(Mutex::new(Queue::default()));
        let state = Arc::clone(&queue);
        let notify = Arc::clone(&self.notify);
        let worker = thread::Builder::new()
            .name(format!("minterm-pty-{id}"))
            .spawn(move || {
                let result = run_io(&events, &mut terminal, &mut signal, pid, &state, |data| {
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
                let status = match result {
                    Ok(true) => shell.wait().unwrap_or_else(|error| error),
                    _ => {
                        shell.terminate();
                        result
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "Session closed".to_owned())
                    }
                };
                if !explicitly_closed {
                    notify(SessionEvent::Exited { id, status });
                }
            })
            .map_err(message)?;
        self.tabs.insert(
            id,
            Session {
                master: pair.master,
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
        self.tabs
            .get(&id)
            .ok_or("Session closed")?
            .master
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
                    .master
                    .process_group_leader()
                    .is_some_and(|pid| pid != session.shell)
                    || crate::agents::has_jobs(session.shell))
        })
    }

    pub fn cwd(&self, id: u32) -> Option<PathBuf> {
        let session = self.tabs.get(&id)?;
        let queue = session.queue.lock().unwrap();
        if queue.closed {
            return None;
        }
        session
            .master
            .process_group_leader()
            .and_then(process_cwd)
            .or_else(|| process_cwd(session.shell))
            .or_else(|| Some(session.start.clone()))
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
    reaped: bool,
}

impl Shell {
    fn wait(&mut self) -> Result<String, String> {
        let status = self.child.wait().map_err(message)?;
        self.reaped = true;
        Ok(status.to_string())
    }

    fn terminate(&mut self) {
        if self.reaped {
            return;
        }
        // Keep the session leader unreaped until our last signal. Its PID cannot
        // be reused as another session ID while we inspect the attached jobs.
        let mut pids = self.attached_jobs();
        pids.push(self.pid);
        self.signal(&pids, libc::SIGHUP);
        thread::sleep(Duration::from_millis(150));
        pids.extend(self.attached_jobs());
        self.signal(&pids, libc::SIGKILL);
        let _ = self.wait();
    }

    fn attached_jobs(&self) -> Vec<i32> {
        self.device.map(crate::agents::tty_pids).unwrap_or_default()
    }

    fn signal(&self, pids: &[i32], signal: i32) {
        for &pid in pids {
            if pid > 0 && unsafe { libc::getsid(pid) } == self.pid {
                unsafe {
                    libc::kill(pid, signal);
                }
            }
        }
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
            matches!(next(&rx), SessionEvent::Exited { id: ID, status } if status.contains('7'))
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
            sessions.tabs[&ID].master.process_group_leader(),
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
            sessions.tabs[&ID].master.process_group_leader(),
            Some(sessions.tabs[&ID].shell)
        );
        assert!(sessions.is_busy(ID), "Exec agent went undetected");
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
            .master
            .process_group_leader()
            .is_none_or(|pid| pid == sessions.tabs[&ID].shell)
        {
            assert!(Instant::now() < deadline, "No foreground job");
            thread::sleep(Duration::from_millis(5));
        }
        let foreground = sessions.tabs[&ID].master.process_group_leader().unwrap();
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
