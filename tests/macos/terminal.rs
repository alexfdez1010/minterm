// Run on macOS with a logged-in GUI session and the Xcode Command Line Tools:
// cargo test --features ui-tests --test macos-ui
// A subprocess owns the AppKit main thread and a hidden window. Its environment,
// repositories and CLI fixtures live in a temporary home.
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

// The fixture CLIs require a controlling terminal and record the literal task.
fn install_agent_fixtures(home: &Path) -> std::path::PathBuf {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    // The Ollama launcher takes its own options and forwards arguments after --.
    fs::write(
        home.join(".bash_profile"),
        r#"export PATH="$HOME/.local/bin:$PATH"
ollama() {
    [[ $1 == launch && $2 == opencode && $3 == --model && $5 == -- ]] || return 64
    local model=$4
    shift 5
    "$HOME/.local/bin/opencode" --model "$model" "$@"
}
alias ocq='ollama launch opencode --model fixture/qwen'
"#,
    )
    .unwrap();
    for name in ["codex", "claude", "opencode"] {
        let cli = bin.join(name);
        fs::write(
            &cli,
            r#"#!/bin/sh
set -eu
[ -t 0 ] && [ -t 1 ] || exit 65
if [ "$#" -eq 0 ]; then
    record=blank
    printf 'Started %s without a task\n' "${0##*/}"
elif [ "${0##*/}" = opencode ] && [ "$1" = --model ]; then
    printf 'Model: %s\n' "$2"
    if [ "$#" -eq 2 ]; then record=custom-empty; else record=custom-task; fi
elif [ "${0##*/}" = codex ] && [ "$1" = -m ]; then
    printf 'Model: %s\n' "$2"
    case "$4" in
        'model_reasoning_effort="low"') record=low ;;
        'model_reasoning_effort="ultra"') record=ultra ;;
        *) exit 64 ;;
    esac
else
    record=${0##*/}
    if [ "$record" != opencode ]; then [ "$#" -eq 2 ] && [ "$1" = -- ] || exit 64; fi
fi
pwd > "$record-cwd"
: > "$record-argv"
for arg do printf '%s\0' "$arg" >> "$record-argv"; done
printf 'Interactive fixture ready\nRunning a command\n'
while IFS= read -r line; do
    [ "$line" != /exit ] || exit 0
    printf 'Fixture reply: %s\n' "$line"
done
"#,
        )
        .unwrap();
        fs::set_permissions(cli, fs::Permissions::from_mode(0o700)).unwrap();
    }
    // Use the local model catalog's listener as a synthetic Docker port mapping.
    // This exercises deduplication across the Docker parser and real lsof output.
    let docker = bin.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
set -eu
[ "$1" = --host ] && [ "$3" = ps ] || exit 64
printf 'fixture-postgres\tpostgres:16\trunning\t127.0.0.1:%s->5432/tcp\n' "$FIXTURE_PORT"
printf 'fixture-cache\tredis:7\trunning\t6379/tcp\n'
printf 'fixture-web\tlocal/web:1\trunning\t127.0.0.1:9091->8080/tcp\n'
printf 'fixture-other\tlocal/other:1\trunning\t127.0.0.1:19991->20001/tcp\n'
printf 'fixture-private\tlocal/worker:1\trunning\t8080/tcp\n'
"#,
    )
    .unwrap();
    fs::set_permissions(docker, fs::Permissions::from_mode(0o700)).unwrap();
    bin
}

// A newly installed second model appears only after Refresh. Both requests
// stay on loopback and exercise the native pane's normal /api/tags path.
fn serve_model_catalog(server: &TcpListener, requests: &mut usize) {
    while let Ok((mut stream, _)) = server.accept() {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut headers = BufReader::new(&stream).lines();
        assert_eq!(headers.next().unwrap().unwrap(), "GET /api/tags HTTP/1.1");
        for line in headers {
            if line.unwrap().is_empty() {
                break;
            }
        }
        *requests += 1;
        let body = if *requests == 1 {
            r#"{"models":[{"name":"fixture-coder:1b"}]}"#
        } else {
            r#"{"models":[{"name":"fixture-coder:1b"},{"name":"fixture-reasoner:2b"}]}"#
        };
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    }
}

fn main() {
    if env::args().nth(1).as_deref() == Some("--app") {
        minterm::launch(env::current_dir().unwrap(), false, |window, _| {
            let script = CString::new(concat!(
                include_str!("preferences.js"),
                "\n",
                include_str!("terminal.js")
            ))
            .unwrap();
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
    let command_dir = home.join("command-target");
    init_repository(&workspace, "main");
    init_repository(&nested, "agent/review");
    init_repository(&command_dir, "command/fixture");
    let fixture_bin = install_agent_fixtures(&home);
    fs::write(
        home.join("extra-profile.toml"),
        r#"
[[commands]]
name = "freshq"
agent = "opencode"
command = "opencode --model fixture/external"
"#,
    )
    .unwrap();
    let models = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    models.set_nonblocking(true).unwrap();
    let mut model_requests = 0;
    let prefs_dir = home.join("Library/Preferences");
    fs::create_dir_all(&prefs_dir).unwrap();
    let prefs_file = prefs_dir.join("com.github.alexfdez1010.minterm.json");
    fs::write(
        &prefs_file,
        serde_json::json!({"ollamaHost": format!("http://{}", models.local_addr().unwrap())})
            .to_string(),
    )
    .unwrap();
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
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("SHELL", "/bin/bash")
        .env("LANG", "en_US.UTF-8")
        .env(
            "FIXTURE_PORT",
            models.local_addr().unwrap().port().to_string(),
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
        serve_model_catalog(&models, &mut model_requests);
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
    assert_eq!(model_requests, 2);
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(prefs_file).unwrap()).unwrap();
    assert_eq!(stored["ollamaModel"], "fixture-reasoner:2b");
    assert_eq!(
        fs::read_to_string(nested.join("tested-cwd"))
            .unwrap()
            .trim(),
        nested.to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(nested.join("codex-cwd"))
            .unwrap()
            .trim_end(),
        nested.to_str().unwrap(),
    );
    assert_eq!(
        fs::read(nested.join("codex-argv")).unwrap(),
        b"--\0Review fixture changes\nDescribe `git status` literally.\0",
    );
    assert_eq!(
        fs::read_to_string(command_dir.join("codex-cwd"))
            .unwrap()
            .trim_end(),
        command_dir.to_str().unwrap(),
    );
    assert_eq!(
        fs::read(command_dir.join("codex-argv")).unwrap(),
        b"--\0touch shell-must-not-run\nReview the current directory\0",
    );
    assert_eq!(
        fs::read_to_string(command_dir.join("opencode-cwd"))
            .unwrap()
            .trim_end(),
        command_dir.to_str().unwrap()
    );
    assert_eq!(
        fs::read(command_dir.join("opencode-argv")).unwrap(),
        b"--prompt=Review the fixture\nKeep `pwd` literal.\0"
    );
    assert_eq!(
        fs::read(nested.join("custom-empty-argv")).unwrap(),
        b"--model\0fixture/qwen\0",
    );
    assert_eq!(
        fs::read_to_string(command_dir.join("custom-task-cwd"))
            .unwrap()
            .trim_end(),
        command_dir.to_str().unwrap()
    );
    assert_eq!(
        fs::read(command_dir.join("custom-task-argv")).unwrap(),
        b"--model\0fixture/qwen\0--prompt=--help review\nKeep $(touch alias-command-ran) literal.\0"
    );
    assert!(!command_dir.join("alias-command-ran").exists());
    assert_eq!(
        fs::read(home.join("external/custom-task-argv")).unwrap(),
        b"--model\0fixture/external\0--prompt=review external edit\0"
    );
    for effort in ["low", "ultra"] {
        assert_eq!(
            fs::read(command_dir.join(format!("{effort}-argv"))).unwrap(),
            format!("-m\0gpt-6.2-sol\0-c\0model_reasoning_effort=\"{effort}\"\0--\0Test the change\nReview this diff.\0").as_bytes()
        );
    }
    assert!(fs::read(nested.join("blank-argv")).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(command_dir.join("blank-cwd"))
            .unwrap()
            .trim_end(),
        command_dir.to_str().unwrap()
    );
    assert!(!command_dir.join("shell-must-not-run").exists());
    assert_eq!(
        fs::read_to_string(command_dir.join("shell-restored")).unwrap(),
        "restored\n"
    );
}
