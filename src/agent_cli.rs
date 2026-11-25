use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(crate) struct AgentCli {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub path: Option<OsString>,
}

impl AgentCli {
    /// Run the command saved in Settings after the user's startup files define
    /// aliases and functions. The directory and all CLI arguments stay in argv.
    pub fn shell_command(shell: &Path, command: &str, cwd: &Path) -> Self {
        let args = if shell.file_name().is_some_and(|name| name == "fish") {
            vec![
                "-lic".into(),
                format!("builtin cd -- \"$argv[1]\"; or exit 1\n{command} $argv[2..-1]").into(),
                "--".into(),
                cwd.into(),
            ]
        } else {
            vec![
                "-lic".into(),
                format!("\\builtin cd -- \"$1\" || exit 1\n\\builtin shift\n{command} \"$@\"")
                    .into(),
                "minterm-agent".into(), // $0 for shell -c
                cwd.into(),
            ]
        };
        Self {
            program: shell.into(),
            args,
            path: env::join_paths(binary_paths()).ok(),
        }
    }
}

// Only bypass the interactive shell for a single, literal executable. A saved
// invocation containing expansions or flags keeps its normal shell semantics.
pub(crate) fn literal_executable(command: &str) -> Option<PathBuf> {
    if !(command.starts_with('/') || command.starts_with("'/")) {
        return None;
    }
    let words = shell_words::split(command).ok()?;
    let [path] = words.as_slice() else {
        return None;
    };
    (Path::new(path).is_absolute() && shell_words::quote(path) == command)
        .then(|| PathBuf::from(path))
}

pub(crate) fn resolve(agent: &str, configured: Option<&Path>) -> Result<AgentCli, String> {
    let name = match agent {
        "codex" => "Codex",
        "claude" => "Claude Code",
        "opencode" => "OpenCode",
        "antigravity" => "Antigravity",
        _ => return Err("Choose Codex, Claude Code, OpenCode or Antigravity.".into()),
    };
    let binary = if agent == "antigravity" { "agy" } else { agent };
    let mut dirs = binary_paths();
    let executable = |file: &Path| {
        fs::metadata(file).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let program = if let Some(file) = configured {
        if !file.is_absolute() || !executable(file) {
            return Err(format!(
                "The {name} executable selected in Settings is unavailable."
            ));
        }
        if let Some(parent) = file.parent() {
            dirs.insert(0, parent.to_owned());
        }
        file.to_owned()
    } else {
        dirs.iter().map(|dir| dir.join(binary)).find(|file| executable(file)).ok_or_else(|| {
            format!("{name} CLI was not found. Install {binary} or select its executable in Settings.")
        })?
    };
    Ok(AgentCli {
        program,
        args: Vec::new(),
        path: env::join_paths(dirs).ok(),
    })
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
            ".opencode/bin",
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
