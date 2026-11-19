mod events;

use crate::credentials::Provider;
use base64::{Engine, engine::general_purpose::STANDARD};
use events::{STARTING, Step, Stream};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, Read, Write},
    mem::MaybeUninit,
    net::Shutdown,
    os::{
        fd::{AsRawFd, RawFd},
        unix::{fs::PermissionsExt, net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum JobEvent {
    Output {
        id: u32,
        data: String,
    },
    Status {
        id: u32,
        state: String,
        detail: String,
    },
    Exited {
        id: u32,
        status: String,
    },
}

type Notify = Arc<dyn Fn(JobEvent) + Send + Sync>;
const BLOCK: usize = 32 * 1024;
const WINDOW: usize = 4;
const RECORD_LIMIT: usize = 8 * 1024 * 1024;

pub struct Jobs {
    jobs: HashMap<u32, Job>,
    retiring: Vec<JoinHandle<()>>,
    notify: Notify,
}
struct Job {
    provider: Provider,
    control: Arc<Control>,
    wake: UnixStream,
    worker: JoinHandle<()>,
}
#[derive(Default)]
struct Control {
    stopped: AtomicBool,
    finished: AtomicBool,
    credits: Mutex<usize>,
    ack: Condvar,
}

impl Jobs {
    pub fn new(notify: impl Fn(JobEvent) + Send + Sync + 'static) -> Self {
        Self {
            notify: Arc::new(notify),
            jobs: HashMap::new(),
            retiring: Vec::new(),
        }
    }

    pub fn start(&mut self, id: u32, agent: &str, cwd: &Path, prompt: &str) -> Result<(), String> {
        let (provider, name, args): (_, _, &[&str]) = match agent {
            "codex" => (Provider::OpenAi, "Codex", &["exec", "--json", "-"]),
            "claude" => (
                Provider::Anthropic,
                "Claude Code",
                &["-p", "--verbose", "--output-format", "stream-json"],
            ),
            _ => return Err("Choose Codex or Claude Code.".into()),
        };
        if self.contains(id) {
            return Err("This run is already open.".into());
        }
        if prompt.trim().is_empty() || prompt.len() > 128 * 1024 {
            return Err("Enter a task of up to 128 KiB.".into());
        }
        let dirs = binary_paths();
        let program = dirs
            .iter()
            .map(|dir| dir.join(agent))
            .find(|file| {
                fs::metadata(file).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
            .ok_or_else(|| {
                format!("{name} CLI was not found. Install {agent} and sign in from a shell.")
            })?;
        let mut command = Command::new(program);
        command.args(args).current_dir(cwd);
        if let Ok(path) = env::join_paths(dirs) {
            command.env("PATH", path);
        }
        self.spawn(id, command, provider, prompt)
            .map_err(|e| format!("Could not start {name}: {e}"))
    }

    fn spawn(
        &mut self,
        id: u32,
        mut command: Command,
        provider: Provider,
        prompt: &str,
    ) -> io::Result<()> {
        let (wake, signal) = UnixStream::pair()?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // setsid creates a group independent of the terminal that launched Minterm.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut process = Process {
            child: command.spawn()?,
            reaped: false,
        };
        let control = Arc::new(Control::default());
        let worker_control = control.clone();
        let notify = self.notify.clone();
        let prompt = prompt.as_bytes().to_vec();
        let worker = thread::Builder::new()
            .name(format!("minterm-run-{id}"))
            .spawn(move || {
                let mut reporter = Reporter::new(id, &worker_control, &notify);
                reporter.status(STARTING, true);
                let mut stream = Stream::default();
                let result = pump(
                    &mut process,
                    &signal,
                    &prompt,
                    provider == Provider::Anthropic,
                    &mut stream,
                    &mut reporter,
                );
                let cancelled = worker_control.stopped.load(Ordering::Acquire);
                let status = process.finish(cancelled || result.is_err());
                let step = if cancelled {
                    Step("stopped", "Run stopped")
                } else if result.is_err() || status.is_err() {
                    Step("failed", "Could not read the agent process")
                } else {
                    stream.finish(status.as_ref().is_ok_and(ExitStatus::success))
                };
                if !cancelled && step.0 != "completed" {
                    reporter.output(step.1);
                }
                reporter.status(step, true);
                worker_control.finished.store(true, Ordering::Release);
                let mut summary = step.1.to_owned();
                if !cancelled
                    && let Ok(status) = status
                    && !status.success()
                {
                    summary = format!("{summary} ({status})");
                }
                notify(JobEvent::Exited {
                    id,
                    status: summary,
                });
            })?;
        self.retiring.retain(|w| !w.is_finished());
        self.jobs.insert(
            id,
            Job {
                provider,
                control,
                wake,
                worker,
            },
        );
        Ok(())
    }

    pub fn contains(&self, id: u32) -> bool {
        self.jobs.contains_key(&id)
    }
    pub fn provider(&self, id: u32) -> Option<Provider> {
        self.jobs.get(&id).map(|job| job.provider)
    }
    pub fn running(&self, id: u32) -> bool {
        self.jobs
            .get(&id)
            .is_some_and(|j| !j.control.finished.load(Ordering::Acquire))
    }
    pub fn ack_output(&self, id: u32) {
        if let Some(job) = self.jobs.get(&id) {
            let mut in_flight = job.control.credits.lock().unwrap();
            *in_flight = in_flight.saturating_sub(1);
            job.control.ack.notify_one();
        }
    }
    pub fn close(&mut self, id: u32) {
        if let Some(job) = self.jobs.remove(&id) {
            let _guard = job.control.credits.lock().unwrap();
            job.control.stopped.store(true, Ordering::Release);
            job.control.ack.notify_all();
            let _ = job.wake.shutdown(Shutdown::Both);
            self.retiring.retain(|worker| !worker.is_finished());
            self.retiring.push(job.worker);
        }
    }
    pub fn shutdown(&mut self) {
        for id in self.jobs.keys().copied().collect::<Vec<_>>() {
            self.close(id);
        }
        for worker in self.retiring.drain(..) {
            let _ = worker.join();
        }
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn binary_paths() -> Vec<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<_> = env::split_paths(&path)
        .filter(|p| p.is_absolute())
        .collect();
    if let Some(home) = env::home_dir() {
        for suffix in [
            ".local/bin",
            ".claude/local",
            ".codex/packages/standalone/current/bin",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
            "Library/pnpm",
        ] {
            dirs.push(home.join(suffix));
        }
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    dirs
}

struct Reporter<'a> {
    id: u32,
    control: &'a Control,
    notify: &'a Notify,
    sent: Option<Step>,
    pending: Option<Step>,
    last: Instant,
    secrets: Vec<String>,
}
impl<'a> Reporter<'a> {
    fn new(id: u32, control: &'a Control, notify: &'a Notify) -> Self {
        let secrets = [
            "OPEN_API_KEY",
            "OPENAI_API_KEY",
            "CODEX_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ]
        .into_iter()
        .filter_map(|name| env::var(name).ok())
        .filter(|s| !s.is_empty())
        .collect();
        Self {
            id,
            control,
            notify,
            sent: None,
            pending: None,
            last: Instant::now(),
            secrets,
        }
    }
    fn status(&mut self, step: Step, force: bool) {
        self.pending = Some(step);
        if self.sent == Some(step) {
            self.pending = None;
            return;
        }
        if !force && self.last.elapsed() < Duration::from_millis(80) {
            return;
        }
        (self.notify)(JobEvent::Status {
            id: self.id,
            state: step.0.into(),
            detail: step.1.into(),
        });
        self.sent = Some(step);
        self.pending = None;
        self.last = Instant::now();
    }
    fn output(&self, text: &str) {
        // An agent response is plain text. In particular OSC, DCS and cursor controls
        // are never forwarded from the child's JSON or stderr into the terminal.
        let mut safe: String = text
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect();
        for secret in &self.secrets {
            safe = safe.replace(secret, "[redacted]");
        }
        let safe = redact_api_keys(&safe).replace('\n', "\r\n") + "\r\n\r\n";
        for block in safe.as_bytes().chunks(BLOCK) {
            let count = self.control.credits.lock().unwrap();
            let mut count = self
                .control
                .ack
                .wait_while(count, |n| {
                    *n >= WINDOW && !self.control.stopped.load(Ordering::Acquire)
                })
                .unwrap();
            if self.control.stopped.load(Ordering::Acquire) {
                return;
            }
            *count += 1;
            drop(count);
            (self.notify)(JobEvent::Output {
                id: self.id,
                data: STANDARD.encode(block),
            });
        }
    }
}

fn redact_api_keys(mut text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    while let Some(offset) = text.find("sk-") {
        out.push_str(&text[..offset]);
        text = &text[offset..];
        let n = text
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            .count();
        out.push_str(if n >= 12 { "[redacted]" } else { &text[..n] });
        text = &text[n..];
    }
    out.push_str(text);
    out
}

struct Process {
    child: Child,
    reaped: bool,
}
impl Process {
    fn exited(&self) -> io::Result<bool> {
        let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init().si_pid() } != 0)
    }
    fn finish(&mut self, stop: bool) -> io::Result<ExitStatus> {
        let pid = self.child.id() as i32;
        if stop {
            let mut descendants = descendants_of(pid);
            signal_session(pid, libc::SIGTERM, &descendants);
            thread::sleep(Duration::from_millis(150));
            descendants.extend(descendants_of(pid));
            signal_session(pid, libc::SIGKILL, &descendants);
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish(true);
        }
    }
}

// Only used on cancellation. Job-control tools can form their own process groups
// in our session. Keep the session leader unreaped while verifying membership.
fn signal_session(leader: i32, sig: i32, descendants: &[(i32, (u64, u64))]) {
    // Some tools create their own sessions. Keep their birth times from before
    // TERM so reparenting is harmless and reused process IDs are never signaled.
    for &(pid, birth) in descendants {
        if process_info(pid).is_some_and(|p| (p.pbi_start_tvsec, p.pbi_start_tvusec) == birth) {
            unsafe {
                libc::kill(pid, sig);
            }
        }
    }
    let size = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if size > 0 {
        let mut pids = vec![0i32; size as usize + 32];
        let n = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                size_of_val(pids.as_slice()) as i32,
            )
        };
        for &pid in pids.iter().take(n.max(0) as usize) {
            if pid != leader && pid > 0 && unsafe { libc::getsid(pid) } == leader {
                unsafe {
                    libc::kill(pid, sig);
                }
            }
        }
    }
    unsafe {
        libc::kill(-leader, sig);
    }
}

fn process_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (n == size).then(|| unsafe { info.assume_init() })
}

fn descendants_of(root: i32) -> Vec<(i32, (u64, u64))> {
    let mut todo = vec![root];
    let mut children = Vec::new();
    while let Some(parent) = todo.pop() {
        let n = unsafe { libc::proc_listchildpids(parent, std::ptr::null_mut(), 0) };
        if n <= 0 {
            continue;
        }
        let mut pids = vec![0; n as usize + 16];
        let n = unsafe {
            libc::proc_listchildpids(
                parent,
                pids.as_mut_ptr().cast(),
                size_of_val(pids.as_slice()) as i32,
            )
        };
        for pid in pids.into_iter().take(n.max(0) as usize) {
            if let Some(p) = process_info(pid)
                && p.pbi_ppid == parent as u32
            {
                children.push((pid, (p.pbi_start_tvsec, p.pbi_start_tvusec)));
                todo.push(pid);
            }
        }
    }
    children
}

fn nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct Lines {
    bytes: Vec<u8>,
    dropping: bool,
}
impl Lines {
    fn push(&mut self, bytes: &[u8], max: usize, mut line: impl FnMut(Option<&[u8]>)) {
        for piece in bytes.split_inclusive(|&b| b == b'\n') {
            let complete = piece.last() == Some(&b'\n');
            if !self.dropping {
                if self.bytes.len() + piece.len() <= max {
                    self.bytes.extend_from_slice(piece);
                } else {
                    self.bytes.clear();
                    self.dropping = true;
                }
            }
            if complete {
                line((!self.dropping).then_some(self.bytes.as_slice()));
                self.bytes.clear();
                self.dropping = false;
            }
        }
    }
}

fn buffered(fd: RawFd) -> io::Result<usize> {
    let mut bytes: libc::c_int = 0;
    if unsafe { libc::ioctl(fd, libc::FIONREAD, &mut bytes) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes.max(0) as usize)
}

fn pump(
    process: &mut Process,
    signal: &UnixStream,
    prompt: &[u8],
    claude: bool,
    stream: &mut Stream,
    report: &mut Reporter<'_>,
) -> io::Result<()> {
    let mut input = process.child.stdin.take();
    let mut out = process.child.stdout.take().expect("agent stdout");
    let mut err = process.child.stderr.take().expect("agent stderr");
    for fd in [
        input.as_ref().unwrap().as_raw_fd(),
        out.as_raw_fd(),
        err.as_raw_fd(),
    ] {
        nonblocking(fd)?;
    }
    let mut sent = 0;
    let mut stdout = Lines::default();
    let mut stderr = Lines::default();
    let mut eof = [false; 2];
    let mut buf = [0u8; BLOCK];
    let mut oversized = false;
    let mut tail = None;
    loop {
        if report.control.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(step) = report.pending {
            report.status(step, false);
        }
        let exited = tail.is_some() || process.exited()?;
        // Descendants can outlive a successful CLI (for example a dev server)
        // and keep its pipes open. Drain exactly what was buffered at CLI exit.
        if exited && tail.is_none() {
            tail = Some([buffered(out.as_raw_fd())?, buffered(err.as_raw_fd())?]);
        }
        let mut polls = [
            (signal.as_raw_fd(), libc::POLLIN),
            (input.as_ref().map_or(-1, AsRawFd::as_raw_fd), libc::POLLOUT),
            (if eof[0] { -1 } else { out.as_raw_fd() }, libc::POLLIN),
            (if eof[1] { -1 } else { err.as_raw_fd() }, libc::POLLIN),
        ]
        .map(|(fd, events)| libc::pollfd {
            fd,
            events,
            revents: 0,
        });
        let timeout = if exited {
            0
        } else if report.pending.is_some() {
            80
        } else {
            300
        };
        let ready = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout) };
        if ready < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if polls[0].revents != 0 {
            return Ok(());
        }
        if polls[1].revents != 0
            && let Some(writer) = &mut input
        {
            match writer.write(&prompt[sent..]) {
                Ok(n) => {
                    sent += n;
                    if sent == prompt.len() || n == 0 {
                        input.take();
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                    input.take();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        let mut progress = false;
        for i in 0..2 {
            if eof[i] || (polls[i + 2].revents == 0 && !exited) {
                continue;
            }
            let limit = tail.map_or(BLOCK, |sizes| sizes[i].min(BLOCK));
            let n = match if limit == 0 {
                Ok(0)
            } else if i == 0 {
                out.read(&mut buf[..limit])
            } else {
                err.read(&mut buf[..limit])
            } {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && exited => 0,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            };
            if let Some(sizes) = &mut tail {
                sizes[i] -= n;
            }
            if n == 0 {
                eof[i] = true;
            } else {
                progress = true;
            }
            // A final record may end at EOF without a trailing newline.
            let bytes = if n == 0 { b"\n".as_slice() } else { &buf[..n] };
            if i == 0 {
                stdout.push(bytes, RECORD_LIMIT, |line| {
                    if let Some(line) = line {
                        let record = stream.parse(line, claude);
                        if let Some(step) = record.step {
                            report.status(step, false);
                        }
                        if let Some(text) = record.text {
                            report.output(&text);
                        }
                    } else if !oversized {
                        oversized = true;
                        report.output("An oversized agent event was omitted.");
                    }
                });
            } else {
                stderr.push(bytes, 16 * 1024, |line| {
                    if let Some(line) = line.and_then(|l| std::str::from_utf8(l).ok()) {
                        stream.diagnostic = events::diagnose(line).or(stream.diagnostic);
                    }
                });
            }
        }
        if exited && (!progress || eof == [true; 2]) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver};

    fn harness() -> (Jobs, Receiver<JobEvent>) {
        let (tx, rx) = mpsc::channel();
        (
            Jobs::new(move |event| {
                let _ = tx.send(event);
            }),
            rx,
        )
    }

    fn script(dir: &Path, contents: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", contents]).current_dir(dir);
        command
    }

    fn transcript(jobs: &Jobs, rx: &Receiver<JobEvent>) -> (String, String, String) {
        let mut text = Vec::new();
        let mut state = String::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).expect("Job event") {
                JobEvent::Output { id, data } => {
                    text.extend(STANDARD.decode(data).unwrap());
                    jobs.ack_output(id);
                }
                JobEvent::Status { state: update, .. } => state = update,
                JobEvent::Exited { status, .. } => {
                    return (String::from_utf8(text).unwrap(), state, status);
                }
            }
        }
    }

    #[test]
    fn task_is_piped_and_only_human_response_crosses_bridge() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = "literal '\"$(exit 13)'\nsecond line\n";
        let command = script(
            tmp.path(),
            r#"
            cat > received
            echo 'debug arguments are private' >&2
            echo '{"type":"item.started","item":{"type":"command_execution","command":"PRIVATE INPUT"}}'
            printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"Line one\nLine two sk-testFAKE1234567890"}}'
            printf '%s' '{"type":"turn.completed"}'
        "#,
        );
        let (mut jobs, rx) = harness();
        jobs.spawn(7, command, Provider::OpenAi, prompt).unwrap();
        let (text, state, _) = transcript(&jobs, &rx);
        assert_eq!(text, "Line one\r\nLine two [redacted]\r\n\r\n");
        assert_eq!(state, "completed");
        assert_eq!(
            fs::read_to_string(tmp.path().join("received")).unwrap(),
            prompt
        );
        assert!(!jobs.running(7));
        assert!(jobs.contains(7));
    }

    #[test]
    fn handles_claude_denials_api_errors_and_duplicate_final_responses() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut jobs, rx) = harness();
        let command = script(
            tmp.path(),
            r#"
            cat >/dev/null
            echo '{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"private"},{"type":"tool_use","name":"Read","input":{"file_path":"private"}}]}}'
            echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}'
            echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello","permission_denials":[]}'
        "#,
        );
        jobs.spawn(1, command, Provider::Anthropic, "task").unwrap();
        let (text, state, _) = transcript(&jobs, &rx);
        assert_eq!(text, "Hello\r\n\r\n");
        assert_eq!(state, "completed");

        let command = script(
            tmp.path(),
            r#"cat >/dev/null; echo '{"type":"result","subtype":"error_during_execution","is_error":true,"permission_denials":[{"tool_input":"private"}]}'"#,
        );
        jobs.spawn(2, command, Provider::Anthropic, "task").unwrap();
        let (text, state, _) = transcript(&jobs, &rx);
        assert_eq!(state, "blocked");
        assert!(text.contains("Permission denied"));

        let command = script(
            tmp.path(),
            "cat >/dev/null; echo 'Not authenticated: very-private-data' >&2; exit 2",
        );
        jobs.spawn(3, command, Provider::OpenAi, "task").unwrap();
        let (text, state, status) = transcript(&jobs, &rx);
        assert_eq!(state, "failed");
        assert!(status.contains("Sign in"));
        assert!(!text.contains("very-private"));
    }

    #[test]
    fn backpressure_is_bounded_and_cancel_stops_descendants_without_blocking_ui() {
        let tmp = tempfile::tempdir().unwrap();
        let command = script(
            tmp.path(),
            r#"
            trap '' TERM
            cat >/dev/null
            /bin/sleep 30 & echo $! > child
            while :; do
                echo '{"type":"item.completed","item":{"type":"agent_message","text":"Testing flow control"}}'
            done
        "#,
        );
        let (mut jobs, rx) = harness();
        jobs.spawn(9, command, Provider::OpenAi, "task").unwrap();
        let mut n = 0;
        while n < WINDOW {
            if matches!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                JobEvent::Output { .. }
            ) {
                n += 1;
            }
        }
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        let pid: i32 = fs::read_to_string(tmp.path().join("child"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
        let started = Instant::now();
        jobs.close(9);
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(!jobs.contains(9));
        jobs.shutdown();
        assert!(
            rx.try_iter()
                .any(|e| matches!(e,JobEvent::Exited{status,..} if status == "Run stopped"))
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_info(pid).is_some_and(|p| p.pbi_status != libc::SZOMB) {
            assert!(Instant::now() < deadline, "A job descendant survived");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn bounded_line_reader_recovers_after_large_tool_payload() {
        let mut lines = Lines::default();
        let mut seen = Vec::new();
        for part in [b"0123456789".as_slice(), b"discard\nOK", b"!\n"] {
            lines.push(part, 8, |line| seen.push(line.map(|v| v.to_vec())));
            assert!(lines.bytes.len() <= 8);
        }
        assert_eq!(seen, vec![None, Some(b"OK!\n".to_vec())]);
    }
}
