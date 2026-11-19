use muda::accelerator::{Key, KeyAccelerator, Modifiers};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub agent: String,
    pub shortcut: String,
    pub new_tab_mode: String,
    pub ollama_host: String,
    pub ollama_model: String,
    codex_path: String,
    claude_path: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            agent: "codex".into(),
            shortcut: "⇧⌘K".into(),
            new_tab_mode: "terminal".into(),
            ollama_host: "http://127.0.0.1:11434".into(),
            ollama_model: String::new(),
            codex_path: String::new(),
            claude_path: String::new(),
        }
    }
}

impl Preferences {
    pub fn executable(&self, agent: &str) -> Option<&Path> {
        let value = match agent {
            "codex" | "codex-ollama" => &self.codex_path,
            "claude" => &self.claude_path,
            _ => return None,
        };
        (!value.is_empty()).then(|| Path::new(value))
    }

    fn set(&mut self, field: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        match field {
            "agent" if matches!(value, "codex" | "claude" | "codex-ollama") => {
                self.agent = value.into()
            }
            "newTabMode" if matches!(value, "terminal" | "agent") => {
                self.new_tab_mode = value.into()
            }
            "shortcut" if accelerator(value).is_some() => self.shortcut = value.into(),
            "ollamaHost" => {
                let uri: wry::http::Uri = value
                    .parse()
                    .map_err(|_| "Enter an HTTP or HTTPS server URL.")?;
                if !matches!(uri.scheme_str(), Some("http" | "https"))
                    || uri.host().is_none()
                    || uri.authority().is_some_and(|a| a.as_str().contains('@'))
                    || uri.query().is_some()
                {
                    return Err(
                        "Enter an HTTP or HTTPS server URL without credentials or a query.".into(),
                    );
                }
                self.ollama_host = native_ollama_host(value).into();
            }
            "ollamaModel" if value.len() <= 256 && !value.chars().any(char::is_control) => {
                self.ollama_model = value.into()
            }
            "codexPath" | "claudePath" if value.is_empty() || Path::new(value).is_absolute() => {
                if field == "claudePath" {
                    self.claude_path = value.into()
                } else {
                    self.codex_path = value.into()
                }
            }
            _ => return Err("This setting is invalid.".into()),
        }
        Ok(())
    }
}

pub fn accelerator(shortcut: &str) -> Option<KeyAccelerator> {
    let letter = shortcut.chars().last()?;
    if !letter.is_ascii_uppercase() {
        return None;
    }
    let prefix = shortcut.strip_suffix(letter)?;
    let mut mods = Modifiers::META;
    let mut rest = prefix;
    for (symbol, flag) in [
        ('⌃', Modifiers::CONTROL),
        ('⌥', Modifiers::ALT),
        ('⇧', Modifiers::SHIFT),
    ] {
        if let Some(next) = rest.strip_prefix(symbol) {
            mods |= flag;
            rest = next;
        }
    }
    (rest == "⌘").then(|| {
        KeyAccelerator::new(
            mods,
            Key::Character(letter.to_ascii_lowercase().to_string()),
        )
    })
}

fn path() -> Option<PathBuf> {
    std::env::home_dir()
        .map(|dir| dir.join("Library/Preferences/com.github.alexfdez1010.minterm.json"))
}

fn native_ollama_host(value: &str) -> &str {
    let host = value.trim_end_matches('/');
    if host
        .split_once("://")
        .is_some_and(|(_, address)| address.ends_with("/v1"))
    {
        &host[..host.len() - 3]
    } else {
        host
    }
}

pub fn read() -> Preferences {
    let mut prefs: Preferences = path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    prefs.ollama_host = native_ollama_host(&prefs.ollama_host).to_owned();
    prefs
}

pub fn update(field: &str, value: &str) -> Result<(), String> {
    let path = path().ok_or("Could not locate the preferences folder.")?;
    let dir = path.parent().unwrap();
    let error = |e: std::io::Error| format!("Could not save preferences. {e}");
    fs::create_dir_all(dir).map_err(error)?;
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .map_err(error)?;
    lock.lock().map_err(error)?;
    let mut prefs = match fs::read(&path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "Could not read existing preferences.")?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
        Err(e) => return Err(error(e)),
    };
    prefs.set(field, value)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(error)?;
    serde_json::to_writer_pretty(&mut temp, &prefs).map_err(|e| e.to_string())?;
    temp.flush().map_err(error)?;
    temp.persist(&path).map_err(|e| error(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_settings_validate_shortcuts_origins_and_cli_paths() {
        let mut prefs = Preferences::default();
        for shortcut in ["⌘J", "⌃⌥⇧⌘L", "⇧⌘K"] {
            assert!(accelerator(shortcut).is_some());
        }
        for shortcut in ["K", "⇧K", "⌘1", "⌘⇧K", "⌘Delete"] {
            assert!(accelerator(shortcut).is_none());
        }
        for url in [
            "file:///tmp",
            "http://user:pass@localhost",
            "https://localhost/?key=x",
        ] {
            assert!(prefs.set("ollamaHost", url).is_err());
        }
        prefs.set("ollamaHost", "http://[::1]:11434/").unwrap();
        assert_eq!(prefs.ollama_host, "http://[::1]:11434");
        prefs
            .set("ollamaHost", "https://example.test/ollama/v1/")
            .unwrap();
        assert_eq!(prefs.ollama_host, "https://example.test/ollama");
        assert_eq!(native_ollama_host("http://v1/"), "http://v1");
        assert!(prefs.set("codexPath", "codex --exec").is_err());
        prefs.set("claudePath", "/home/My Tools/claude").unwrap();
        assert_eq!(
            prefs.executable("claude"),
            Some(Path::new("/home/My Tools/claude"))
        );
    }
}
