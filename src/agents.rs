use serde::Serialize;
use std::{collections::HashMap, mem::MaybeUninit, path::Path, ptr};

#[derive(Debug, Serialize)]
pub struct Agent {
    pub id: u32,
    pub pid: u32,
    pub name: String,
    pub foreground: bool,
}

fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    // libproc returns the number of initialized bytes, or zero on failure.
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

pub(crate) fn tty_pids(device: u32) -> Vec<i32> {
    const PROC_TTY_ONLY: u32 = 3;
    let bytes = unsafe { libc::proc_listpids(PROC_TTY_ONLY, device, ptr::null_mut(), 0) };
    if bytes <= 0 {
        return Vec::new();
    }
    let mut pids = vec![0i32; bytes as usize / size_of::<i32>() + 16];
    let written = unsafe {
        libc::proc_listpids(
            PROC_TTY_ONLY,
            device,
            pids.as_mut_ptr().cast(),
            size_of_val(pids.as_slice()) as i32,
        )
    };
    pids.truncate(written.max(0) as usize / size_of::<i32>());
    pids
}

fn cli_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "codex" => "Codex",
        "claude" => "Claude Code",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode",
        "agy" => "Antigravity",
        "aider" => "Aider",
        "copilot" | "copilot-cli" => "GitHub Copilot",
        "qwen" => "Qwen Code",
        "crush" => "Crush",
        "cursor-agent" => "Cursor Agent",
        _ => return None,
    })
}

fn script_name(script: &str) -> Option<&'static str> {
    let path = Path::new(script);
    // Package managers also invoke shims from a prefix's bin/ or node_modules/.bin/.
    if matches!(path.parent()?.file_name()?.to_str(), Some("bin" | ".bin"))
        && let Some(name) = cli_name(path.file_name()?.to_str()?)
    {
        return Some(name);
    }
    [
        ("@openai/codex/bin/codex.js", "Codex"),
        ("@anthropic-ai/claude-code/cli.js", "Claude Code"),
        ("@google/gemini-cli/dist/index.js", "Gemini CLI"),
        ("@google/gemini-cli/bundle/gemini.js", "Gemini CLI"),
        ("@github/copilot/index.js", "GitHub Copilot"),
        ("@github/copilot/npm-loader.js", "GitHub Copilot"),
        ("opencode-ai/bin/opencode", "OpenCode"),
        ("@qwen-code/qwen-code/dist/index.js", "Qwen Code"),
    ]
    .iter()
    .find_map(|(suffix, name)| path.ends_with(suffix).then_some(*name))
}

fn interpreter_name(exe: &str) -> bool {
    matches!(exe, "node" | "bun" | "python" | "Python")
        || exe.strip_prefix("python").is_some_and(|version| {
            !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        })
}

// KERN_PROCARGS2 starts with argc and the executable path, followed by padding,
// NUL-delimited argv, then the environment. Only inspect the invocation prefix.
fn argument_prefix(bytes: &[u8]) -> Option<impl Iterator<Item = &str>> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    let fields = &bytes[4..];
    let after_exe = &fields[fields.iter().position(|&b| b == 0)?..];
    let args = &after_exe[after_exe.iter().position(|&b| b != 0)?..];
    Some(
        args.split(|&b| b == 0)
            .take(usize::try_from(argc).ok()?.min(12))
            .map(|b| std::str::from_utf8(b).unwrap_or("")),
    )
}

fn interpreter_agent<'a>(exe: &str, args: impl Iterator<Item = &'a str>) -> Option<&'static str> {
    let mut args = args.skip(1);
    while let Some(word) = args.next() {
        match word {
            "-m" if exe.to_ascii_lowercase().starts_with("python") => {
                return matches!(args.next(), Some("aider" | "aider.main")).then_some("Aider");
            }
            "--enable-source-maps" | "--no-warnings" | "--no-deprecation" | "-I" | "-u" | "-B" => {}
            "--" => return agent_mode(script_name(args.next()?)?, args.next()),
            s if s.starts_with('-') => return None,
            script => return agent_mode(script_name(script)?, args.next()),
        }
    }
    None
}

fn agent_mode(name: &'static str, first: Option<&str>) -> Option<&'static str> {
    match (name, first) {
        ("Codex", Some("app-server" | "mcp-server" | "mcp"))
        | ("Claude Code", Some("mcp"))
        | ("OpenCode", Some("serve" | "web" | "acp"))
        | ("Antigravity", Some("acp" | "remote-control")) => None,
        _ => Some(name),
    }
}

fn read_arguments(pid: i32, scratch: &mut Vec<u8>) -> Option<&[u8]> {
    if scratch.is_empty() {
        let max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        if max <= 0 {
            return None;
        }
        scratch.resize(max as usize, 0);
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = scratch.len();
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            scratch.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then(|| &scratch[..size])
}

fn process_agent(pid: i32, scratch: &mut Vec<u8>) -> Option<&'static str> {
    let mut path = [0; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if len <= 0 {
        return None;
    }
    let path = Path::new(std::str::from_utf8(&path[..len as usize]).ok()?);
    let exe = path.file_name()?.to_str()?;
    if interpreter_name(exe) {
        return interpreter_agent(exe, argument_prefix(read_arguments(pid, scratch)?)?);
    }
    let name = cli_name(exe).or_else(|| {
        // Anthropic's native installer links claude to .../claude/versions/<version>.
        (path.parent()?.ends_with("claude/versions")
            && exe.starts_with(|c: char| c.is_ascii_digit()))
        .then_some("Claude Code")
    })?;
    if !matches!(name, "Codex" | "Claude Code" | "OpenCode" | "Antigravity") {
        return Some(name);
    }
    let first = read_arguments(pid, scratch)
        .and_then(argument_prefix)
        .and_then(|mut args| args.nth(1));
    agent_mode(name, first)
}

struct Member {
    parent: u32,
    name: Option<&'static str>,
    foreground: bool,
}

fn outermost(pid: u32, name: &str, members: &HashMap<u32, Member>) -> bool {
    let mut parent = members[&pid].parent;
    for _ in 0..members.len() {
        let Some(ancestor) = members.get(&parent) else {
            break;
        };
        if ancestor.name == Some(name) {
            return false;
        }
        parent = ancestor.parent;
    }
    true
}

fn owned_session(shell: i32) -> Option<libc::proc_bsdinfo> {
    let root = bsd_info(shell)?;
    // Minterm owns the PTY session leader. Reject a PID now owned by another app.
    (root.pbi_ppid == std::process::id()
        && root.pbi_status != libc::SZOMB
        && unsafe { libc::getsid(shell) } == shell)
        .then_some(root)
}

fn members_of(shell: i32, device: u32) -> impl Iterator<Item = (i32, libc::proc_bsdinfo)> {
    tty_pids(device).into_iter().filter_map(move |pid| {
        let info = bsd_info(pid)?;
        (info.e_tdev == device
            && info.pbi_status != libc::SZOMB
            && unsafe { libc::getsid(pid) } == shell)
            .then_some((pid, info))
    })
}

/// Job control puts the shell back in the foreground while background jobs
/// run. An agent launched via exec also keeps the shell's PID and group.
pub(crate) fn has_jobs(shell: i32) -> bool {
    let Some(root) = owned_session(shell) else {
        return false;
    };
    members_of(shell, root.e_tdev).any(|(pid, _)| pid != shell)
        || process_agent(shell, &mut Vec::new()).is_some()
}

pub fn discover(shells: &[(u32, i32)]) -> Vec<Agent> {
    let mut agents = Vec::new();
    let mut scratch = Vec::new();
    for &(id, shell) in shells {
        let Some(root) = owned_session(shell) else {
            continue;
        };
        let mut members = HashMap::new();
        for (pid, info) in members_of(shell, root.e_tdev) {
            // Other CLIs still count as running jobs if they replace the login shell.
            // Only the agents supported by Minterm appear in the inspector.
            let name = process_agent(pid, &mut scratch).filter(|name| {
                matches!(*name, "Codex" | "Claude Code" | "OpenCode" | "Antigravity")
            });
            // Check PID identity again after inspecting an apparent agent's executable.
            if name.is_some()
                && !bsd_info(pid).is_some_and(|current| {
                    (current.pbi_start_tvsec, current.pbi_start_tvusec)
                        == (info.pbi_start_tvsec, info.pbi_start_tvusec)
                })
            {
                continue;
            }
            members.insert(
                info.pbi_pid,
                Member {
                    parent: info.pbi_ppid,
                    name,
                    foreground: info.pbi_pgid == root.e_tpgid,
                },
            );
        }
        if !bsd_info(shell).is_some_and(|current| {
            (current.pbi_start_tvsec, current.pbi_start_tvusec)
                == (root.pbi_start_tvsec, root.pbi_start_tvusec)
        }) {
            continue;
        }
        for (&pid, member) in &members {
            if let Some(name) = member.name
                && outermost(pid, name, &members)
            {
                agents.push(Agent {
                    id,
                    pid,
                    name: name.to_owned(),
                    foreground: member.foreground,
                });
            }
        }
    }
    agents.sort_by_key(|a| (a.id, !a.foreground, a.pid));
    agents
}

#[cfg(test)]
pub(crate) fn compile_agent_fixture(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("agent.c");
    let program = dir.join("codex");
    // Creating the marker proves the loader has actually entered main.
    std::fs::write(
        &source,
        r#"
        #include <fcntl.h>
        #include <unistd.h>
        int main(int argc, char **argv) {
            if (argc != 2) return 1;
            int fd = open(argv[1], O_CREAT | O_EXCL | O_WRONLY, 0600);
            if (fd < 0) return 1;
            close(fd);
            sleep(10);
            return 0;
        }
    "#,
    )
    .unwrap();
    assert!(
        std::process::Command::new("/usr/bin/cc")
            .arg(source)
            .arg("-o")
            .arg(&program)
            .status()
            .unwrap()
            .success()
    );
    program
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_entrypoints_without_searching_prompt_or_environment() {
        let mut raw = 3i32.to_ne_bytes().to_vec();
        raw.extend_from_slice(b"/usr/bin/node\0\0\0node\0--enable-source-maps\0/usr/local/lib/node_modules/@google/gemini-cli/dist/index.js\0ENV=secret\0");
        assert_eq!(
            interpreter_agent("node", argument_prefix(&raw).unwrap()),
            Some("Gemini CLI")
        );
        assert_eq!(
            interpreter_agent("node", ["node", "-e", "/usr/local/bin/codex"].into_iter()),
            None
        );
        assert_eq!(
            interpreter_agent(
                "node",
                ["node", "/usr/bin/npm", "exec", "codex"].into_iter()
            ),
            None
        );
        assert_eq!(
            interpreter_agent("python3", ["python3", "-m", "aider"].into_iter()),
            Some("Aider")
        );
        let mut raw = 1i32.to_ne_bytes().to_vec();
        raw.extend_from_slice(b"/bin/node\0\0node\0/usr/local/bin/codex\0");
        assert_eq!(
            interpreter_agent("node", argument_prefix(&raw).unwrap()),
            None
        );
    }

    #[test]
    fn deduplicates_same_agent_ancestors_across_an_unrecognized_wrapper() {
        let members = [
            (5, 1, Some("Codex")),
            (6, 5, None),
            (7, 6, Some("Codex")),
            (8, 5, Some("Aider")),
        ]
        .map(|(pid, parent, name)| {
            (
                pid,
                Member {
                    parent,
                    name,
                    foreground: true,
                },
            )
        })
        .into_iter()
        .collect();
        assert!(outermost(5, "Codex", &members));
        assert!(!outermost(7, "Codex", &members));
        assert!(outermost(8, "Aider", &members));
    }

    #[test]
    fn replacing_the_shell_with_an_unlisted_agent_still_protects_the_tab() {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let tmp = tempfile::tempdir().unwrap();
        let fixture = compile_agent_fixture(tmp.path());
        for (name, listed) in [
            ("aider", None),
            ("opencode", Some("OpenCode")),
            ("agy", Some("Antigravity")),
        ] {
            let program = tmp.path().join(name);
            std::fs::copy(&fixture, &program).unwrap();
            let ready = tmp.path().join(format!("{name}-ready"));
            // An exec'd CLI keeps the shell PID and has no distinct child job.
            let pty = native_pty_system().openpty(PtySize::default()).unwrap();
            let mut cmd = CommandBuilder::new(program);
            cmd.arg(&ready);
            let mut child = pty.slave.spawn_command(cmd).unwrap();
            drop(pty.slave);
            let pid = child.process_id().unwrap() as i32;
            let deadline = Instant::now() + Duration::from_secs(3);
            while !ready.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            let rows = discover(&[(1, pid)]);
            let busy = has_jobs(pid);
            drop(pty.master);
            let _ = child.kill();
            let _ = child.wait();
            assert!(
                ready.is_file() && busy,
                "{name} must be protected when closing"
            );
            assert_eq!(
                rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
                listed.into_iter().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn finds_foreground_and_background_jobs_only_on_the_owned_pty() {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let tmp = tempfile::tempdir().unwrap();
        let program = compile_agent_fixture(tmp.path());
        std::fs::copy(program, tmp.path().join("claude")).unwrap();
        let entered = || {
            ["fg-ready", "bg-ready"]
                .iter()
                .all(|name| tmp.path().join(name).is_file())
        };
        let pty = native_pty_system().openpty(PtySize::default()).unwrap();
        let mut cmd = CommandBuilder::new("/bin/zsh");
        cmd.args([
            "-f",
            "-i",
            "-c",
            "./claude bg-ready & ./codex fg-ready; wait",
        ]);
        cmd.cwd(tmp.path());
        let mut child = pty.slave.spawn_command(cmd).unwrap();
        drop(pty.slave);
        let shell = child.process_id().unwrap() as i32;
        let deadline = Instant::now() + Duration::from_secs(3);
        let found = loop {
            let found = discover(&[(17, shell), (18, std::process::id() as i32)]);
            if (entered() && found.len() == 2) || Instant::now() >= deadline {
                break found;
            }
            thread::sleep(Duration::from_millis(20));
        };
        drop(pty.master);
        let _ = child.kill();
        let _ = child.wait();
        assert!(entered(), "Both agent fixtures must reach main");
        assert_eq!(
            found
                .iter()
                .map(|a| (a.id, a.name.as_str(), a.foreground))
                .collect::<Vec<_>>(),
            [(17, "Codex", true), (17, "Claude Code", false)]
        );
    }
}
