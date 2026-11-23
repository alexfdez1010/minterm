// Run on macOS with a logged-in GUI session and the Xcode Command Line Tools:
// cargo test --features ui-tests --test macos-ui
// A subprocess owns the AppKit main thread and a hidden window. Its environment,
// repositories and CLI fixtures live in a temporary home.
use std::{
    env,
    ffi::{CString, c_char, c_void},
    fs,
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
    // Map the test's real loopback listener to PostgreSQL's container port.
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

fn main() {
    if env::args().nth(1).as_deref() == Some("--app") {
        let restored_split = match env::var("QA_RESTORED_VIEW").as_deref() {
            Ok("split-view") => "true",
            Ok("tab-view") => "false",
            _ => "null",
        };
        minterm::launch(env::current_dir().unwrap(), false, |window, _| {
            let script = CString::new(format!(
                "const restoredSplit = {restored_split};\n{}\n{}",
                include_str!("preferences.js"),
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
shortcut = "Ctrl+R"

[[commands]]
name = "legacy"
agent = "claude"
command = "claude"
shortcut = "Enter+G"
"#,
    )
    .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
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

    let mut app = Command::new(env::current_exe().unwrap());
    app.arg("--app")
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("SHELL", "/bin/bash")
        .env("LANG", "en_US.UTF-8")
        .env(
            "FIXTURE_PORT",
            listener.local_addr().unwrap().port().to_string(),
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
        .current_dir(&workspace);
    let passed = home.join("smoke-passed");
    for view in ["initial", "split-view", "tab-view"] {
        let mut child = app.env("QA_RESTORED_VIEW", view).spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "Window test exited with {status}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Timed out waiting for the hidden terminal integration test");
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(passed.is_file(), "Window test did not finish for {view}");
        fs::remove_file(&passed).unwrap();
    }
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
    assert_eq!(
        fs::read(command_dir.join("low-argv")).unwrap(),
        b"-m\0gpt-6.2-sol\0-c\0model_reasoning_effort=\"low\"\0--\0Test the change\nReview this diff.\0"
    );
    assert_eq!(
        fs::read(command_dir.join("ultra-argv")).unwrap(),
        b"-m\0gpt-6.2-sol\0-c\0model_reasoning_effort=\"ultra\"\0"
    );
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
