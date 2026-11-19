use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(crate) struct AgentCli {
    pub program: PathBuf,
    pub path: Option<OsString>,
}

pub(crate) fn resolve(agent: &str, configured: Option<&Path>) -> Result<AgentCli, String> {
    let name = match agent {
        "codex" => "Codex",
        "claude" => "Claude Code",
        _ => return Err("Choose Codex or Claude Code.".into()),
    };
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
        dirs.iter().map(|dir| dir.join(agent)).find(|file| executable(file)).ok_or_else(|| {
            format!("{name} CLI was not found. Install {agent} or select its executable in Settings.")
        })?
    };
    Ok(AgentCli {
        program,
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
