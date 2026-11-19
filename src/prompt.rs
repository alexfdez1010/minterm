use portable_pty::CommandBuilder;
use std::{
    ffi::OsString,
    fs::{self, Permissions},
    io,
    os::unix::fs::PermissionsExt,
    path::Path,
};
use tempfile::{Builder, TempDir};

/// Startup files live for the lifetime of this app's sessions. Each shell
/// sources its integration normally, without typing into the PTY or its history.
#[derive(Default)]
pub(crate) struct ShellIntegration {
    directory: Option<TempDir>,
}

impl ShellIntegration {
    pub(crate) fn configure(&mut self, cmd: &mut CommandBuilder) -> io::Result<()> {
        if cmd
            .get_env("MINTERM_PROMPT")
            .is_some_and(|value| value == "0")
            && cmd
                .get_env("MINTERM_AUTOSUGGEST")
                .is_some_and(|value| value == "0")
        {
            return Ok(());
        }
        let shell = cmd.get_shell();
        let name = Path::new(&shell).file_name().and_then(|name| name.to_str());
        if !matches!(name, Some("bash" | "zsh")) {
            return Ok(());
        }
        let directory = match self.directory.as_ref() {
            Some(existing) => existing,
            None => self.directory.insert(install()?),
        };
        cmd.env("MINTERM_SHELL_INTEGRATION", directory.path());
        if name == Some("zsh") {
            if let Some(original) = cmd.get_env("ZDOTDIR").map(OsString::from) {
                cmd.env("MINTERM_ORIGINAL_ZDOTDIR", original);
            } else {
                cmd.env_remove("MINTERM_ORIGINAL_ZDOTDIR");
            }
            cmd.env("ZDOTDIR", directory.path().join("zsh"));
        } else {
            // Bash's startup files can extend this command or replace it with
            // their own prompt framework, as they would in any login shell.
            const BASH_HOOK: &str = concat!(
                "__minterm_last_exit=$?; ",
                "if ! builtin declare -F __minterm_bash_prompt >/dev/null; then ",
                "builtin source \"$MINTERM_SHELL_INTEGRATION/bash/prompt.bash\"; fi; ",
                "__minterm_bash_prompt \"$__minterm_last_exit\"",
            );
            let mut hook = OsString::from(BASH_HOOK);
            if let Some(original) = cmd.get_env("PROMPT_COMMAND").filter(|s| !s.is_empty()) {
                // A Minterm instance can itself be launched from an integrated shell.
                if original
                    .as_encoded_bytes()
                    .starts_with(BASH_HOOK.as_bytes())
                {
                    return Ok(());
                }
                hook.push("\n");
                hook.push(original);
            }
            cmd.env("PROMPT_COMMAND", hook);
        }
        Ok(())
    }
}

fn install() -> io::Result<TempDir> {
    let directory = Builder::new()
        .prefix("minterm-shell-")
        .permissions(Permissions::from_mode(0o700))
        .tempdir()?;
    let root = directory.path();
    fs::create_dir(root.join("bash"))?;
    fs::create_dir(root.join("zsh"))?;
    for (name, contents) in [
        ("git.sh", include_bytes!("../shell/git.sh").as_slice()),
        (
            "zsh/suggest.zsh",
            include_bytes!("../shell/zsh/suggest.zsh").as_slice(),
        ),
        (
            "bash/prompt.bash",
            include_bytes!("../shell/bash/prompt.bash").as_slice(),
        ),
        (
            "zsh/.zshenv",
            include_bytes!("../shell/zsh/.zshenv").as_slice(),
        ),
    ] {
        fs::write(root.join(name), contents)?;
    }
    Ok(directory)
}
