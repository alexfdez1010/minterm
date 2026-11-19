// Run on macOS with a logged-in GUI session and the Xcode Command Line Tools:
// cargo test --features ui-tests --test macos-ui
// A subprocess owns the AppKit main thread and a hidden window. Its environment,
// repositories, agent fixture and credential directory live in a temporary home.
use std::{
    env,
    ffi::{CString, c_char, c_void},
    fs,
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
    fs::create_dir(workspace.join("bin")).unwrap();
    // macOS may reject a renamed copy of a signed system executable.
    let fixture = home.join("fixture.c");
    fs::write(
        &fixture,
        "#include <unistd.h>\nint main(void) { sleep(40); }\n",
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
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
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
    assert_eq!(
        fs::read_to_string(nested.join("tested-cwd"))
            .unwrap()
            .trim(),
        nested.to_str().unwrap()
    );
    assert!(
        home.join("smoke-passed").is_file(),
        "Window test did not finish"
    );
}
