// Run on macOS with a logged-in GUI session and the Xcode Command Line Tools:
// cargo test --features ui-tests --test macos-ui
// A subprocess owns the AppKit main thread and a hidden window. Its environment,
// repositories, agent fixture and credential directory live in a temporary home.
use std::{
    env,
    ffi::{CString, c_char, c_void},
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use tao::platform::macos::WindowExtMacOS;

unsafe extern "C" {
    fn qa_install(window: *mut c_void, script: *const c_char);
}

fn init_repository(dir: &Path, branch: &str) {
    fs::create_dir_all(dir).unwrap();
    assert!(
        Command::new("/usr/bin/git")
            .env_clear()
            .env("HOME", dir)
            .args(["init", "--quiet", "-b", branch])
            .arg(dir)
            .status()
            .unwrap()
            .success()
    );
}

// Model the Codex CLI boundary. A tool result and the final reply come from
// distinct JSONL records, just as they do in a non-interactive Codex session.
fn install_agent_fixtures(home: &Path) -> std::path::PathBuf {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = bin.join("codex");
    fs::write(&executable, r#"#!/bin/sh
set -eu
if [ "$#" -ne 3 ] || [ "$1" != exec ] || [ "$2" != --json ] || [ "$3" != - ]; then
    echo "Expected codex exec --json -" >&2
    exit 64
fi
cat > "$HOME/managed-stdin"
pwd > "$HOME/managed-cwd"
mkfifo "$HOME/finish-tool"
cat <<'JSONL'
{"type":"thread.started","thread_id":"fixture-thread"}
{"type":"turn.started"}
{"type":"item.started","item":{"id":"step-1","type":"command_execution","command":"pwd","aggregated_output":"","exit_code":null,"status":"in_progress"}}
JSONL
# Keep the tool step active until the test resumes it from the source shell.
read -r release < "$HOME/finish-tool"
cat <<'JSONL'
{"type":"item.completed","item":{"id":"step-1","type":"command_execution","command":"pwd","aggregated_output":"nested","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"step-2","type":"agent_message","text":"Fixture review complete."}}
{"type":"turn.completed","usage":{"input_tokens":7,"cached_input_tokens":0,"output_tokens":4}}
JSONL
"#).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let claude = bin.join("claude");
    fs::write(
        &claude,
        r#"#!/bin/sh
set -eu
cat > /dev/null
cat <<'JSONL'
{"type":"system","subtype":"init"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Read fixture status."}]}}
{"type":"result","subtype":"success","is_error":false,"result":"Claude fixture complete."}
JSONL
"#,
    )
    .unwrap();
    fs::set_permissions(claude, fs::Permissions::from_mode(0o700)).unwrap();
    bin
}

// Reject CONNECT locally, before TLS handshaking or transmitting any API data.
fn reject_proxy_requests(proxy: &TcpListener, requests: &mut Vec<String>) {
    while let Ok((mut stream, _)) = proxy.accept() {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = String::new();
        BufReader::new(&stream).read_line(&mut request).unwrap();
        requests.push(request.trim_end().to_owned());
        stream
            .write_all(b"HTTP/1.1 502 Test proxy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    }
}

fn main() {
    if env::args().nth(1).as_deref() == Some("--app") {
        minterm::launch(env::current_dir().unwrap(), false, |window, _| {
            let script = CString::new(include_str!("terminal.js")).unwrap();
            // The driver copies the script while the two arguments are valid.
            unsafe {
                qa_install(window.ns_window(), script.as_ptr());
            }
        })
        .unwrap();
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap();
    let workspace = home.join("workspace");
    let nested = workspace.join("nested");
    init_repository(&workspace, "main");
    init_repository(&nested, "agent/review");
    let fixture_bin = install_agent_fixtures(&home);
    let proxy = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    proxy.set_nonblocking(true).unwrap();
    let mut requests = Vec::new();
    fs::create_dir(workspace.join("bin")).unwrap();
    // macOS may reject a renamed copy of a signed system executable.
    let fixture = home.join("fixture.c");
    fs::write(
        &fixture,
        r#"#include <stdio.h>
#include <unistd.h>
int main(void) {
    puts("Do you want to proceed? [y/n]");
    fflush(stdout);
    sleep(40);
}
"#,
    )
    .unwrap();
    assert!(
        Command::new("/usr/bin/cc")
            .arg(&fixture)
            .arg("-o")
            .arg(workspace.join("bin/codex"))
            .status()
            .unwrap()
            .success()
    );

    let mut app = Command::new(env::current_exe().unwrap())
        .arg("--app")
        .env_clear()
        .env("HOME", &home)
        .env("SHELL", "/bin/bash")
        .env("LANG", "en_US.UTF-8")
        .env("ANTHROPIC_API_KEY", "fixture-not-a-real-key")
        .env(
            "https_proxy",
            format!("http://{}", proxy.local_addr().unwrap()),
        )
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", fixture_bin.display()),
        )
        .env("BASH_SILENCE_DEPRECATION_WARNING", "1")
        .env(
            "DOCKER_HOST",
            format!("unix://{}/no-docker.sock", home.display()),
        )
        .current_dir(&workspace)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        reject_proxy_requests(&proxy, &mut requests);
        if let Some(status) = app.try_wait().unwrap() {
            assert!(status.success(), "Window test exited with {status}");
            break;
        }
        if Instant::now() >= deadline {
            let _ = app.kill();
            let _ = app.wait();
            panic!("Timed out waiting for the hidden terminal integration test");
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        home.join("smoke-passed").is_file(),
        "Window test did not finish"
    );
    assert_eq!(requests, ["CONNECT api.anthropic.com:443 HTTP/1.1"]);
    assert_eq!(
        fs::read_to_string(nested.join("tested-cwd"))
            .unwrap()
            .trim(),
        nested.to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(home.join("managed-cwd"))
            .unwrap()
            .trim_end(),
        nested.to_str().unwrap(),
    );
    assert_eq!(
        fs::read_to_string(home.join("managed-stdin")).unwrap(),
        "Review fixture changes\nDescribe `git status` literally.",
    );
}
