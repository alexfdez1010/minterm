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
    pub codex_ollama: bool,
    pub new_tab_mode: String,
    pub ollama_host: String,
    pub ollama_model: String,
    codex_path: String,
    claude_path: String,
    opencode_path: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            agent: "codex".into(),
            shortcut: "⌘K".into(),
            codex_ollama: false,
            new_tab_mode: "terminal".into(),
            ollama_host: "http://127.0.0.1:11434".into(),
            ollama_model: String::new(),
            codex_path: String::new(),
            claude_path: String::new(),
            opencode_path: String::new(),
        }
    }
}

impl Preferences {
    pub fn executable(&self, agent: &str) -> Option<&Path> {
        let value = match agent {
            "codex" => &self.codex_path,
            "claude" => &self.claude_path,
            "opencode" => &self.opencode_path,
            _ => return None,
        };
        (!value.is_empty()).then(|| Path::new(value))
    }
}

fn validated_value<'a>(field: &str, value: &'a str) -> Result<&'a str, String> {
    let value = value.trim();
    match field {
        "agent" if matches!(value, "codex" | "claude" | "opencode") => Ok(value),
        "newTabMode" if matches!(value, "terminal" | "agent") => Ok(value),
        "codexOllama" if matches!(value, "true" | "false") => Ok(value),
        "shortcut" if accelerator(value).is_some() => Ok(value),
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
            Ok(native_ollama_host(value))
        }
        "ollamaModel" if value.len() <= 256 && !value.chars().any(char::is_control) => Ok(value),
        "codexPath" | "claudePath" | "opencodePath"
            if value.is_empty() || Path::new(value).is_absolute() =>
        {
            Ok(value)
        }
        _ => Err("This setting is invalid.".into()),
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
    path()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| load(&bytes).ok())
        .unwrap_or_default()
}

fn migrate(saved: &mut serde_json::Map<String, serde_json::Value>) {
    if saved.contains_key("formatVersion") {
        return;
    }
    // Unversioned files stored every default. A recorded ⇧⌘K was identical
    // to the old default. Both adopt ⌘K once, then explicit choices persist.
    if saved.get("shortcut").and_then(|v| v.as_str()) == Some("⇧⌘K") {
        saved.remove("shortcut");
    }
    if saved.get("agent").and_then(|v| v.as_str()) == Some("codex-ollama") {
        saved.insert("agent".into(), "codex".into());
        saved.entry("codexOllama").or_insert(true.into());
    }
    saved.insert("formatVersion".into(), 1.into());
}

fn load(bytes: &[u8]) -> Result<Preferences, serde_json::Error> {
    let mut saved = serde_json::from_slice(bytes)?;
    migrate(&mut saved);
    let mut prefs: Preferences = serde_json::from_value(serde_json::Value::Object(saved))?;
    prefs.ollama_host = native_ollama_host(&prefs.ollama_host).to_owned();
    Ok(prefs)
}

pub fn update(field: &str, value: &str) -> Result<(), String> {
    let path = path().ok_or("Could not locate the preferences folder.")?;
    update_at(&path, field, value)
}

fn update_at(path: &Path, field: &str, value: &str) -> Result<(), String> {
    let value = validated_value(field, value)?;
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
    let mut saved: serde_json::Map<String, serde_json::Value> = match fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "Could not read existing preferences.")?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(e) => return Err(error(e)),
    };
    migrate(&mut saved);
    let value = if field == "codexOllama" {
        (value == "true").into()
    } else {
        value.into()
    };
    saved.insert(field.into(), value);
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(error)?;
    serde_json::to_writer_pretty(&mut temp, &saved).map_err(|e| e.to_string())?;
    temp.flush().map_err(error)?;
    temp.persist(path).map_err(|e| error(e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_settings_validate_shortcuts_origins_and_cli_paths() {
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
            assert!(validated_value("ollamaHost", url).is_err());
        }
        assert_eq!(
            validated_value("ollamaHost", "http://[::1]:11434/").unwrap(),
            "http://[::1]:11434"
        );
        assert_eq!(
            validated_value("ollamaHost", "https://example.test/ollama/v1/").unwrap(),
            "https://example.test/ollama"
        );
        assert_eq!(native_ollama_host("http://v1/"), "http://v1");
        assert!(validated_value("codexPath", "codex --exec").is_err());
        assert_eq!(
            validated_value("claudePath", "/home/My Tools/claude").unwrap(),
            "/home/My Tools/claude"
        );
    }

    #[test]
    fn switching_agents_keeps_the_legacy_ollama_configuration_and_custom_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let old = br#"{"agent":"codex-ollama","ollamaModel":"coder:7b","codexPath":"/tmp/my codex","claudePath":"/tmp/claude","futureSetting":9}"#;
        fs::write(&path, old).unwrap();
        let read = || load(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(read().agent, "codex");
        assert!(read().codex_ollama);
        update_at(&path, "agent", "opencode").unwrap();
        update_at(&path, "opencodePath", "/opt/Open Code/bin/client").unwrap();
        let prefs = read();
        assert_eq!(prefs.agent, "opencode");
        assert!(prefs.codex_ollama);
        assert_eq!(prefs.ollama_model, "coder:7b");
        assert_eq!(prefs.executable("codex"), Some(Path::new("/tmp/my codex")));
        assert_eq!(
            prefs.executable("opencode"),
            Some(Path::new("/opt/Open Code/bin/client"))
        );
        update_at(&path, "codexOllama", "false").unwrap();
        assert!(!read().codex_ollama);
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["futureSetting"], 9);
    }

    #[test]
    fn legacy_shortcuts_migrate_once_and_new_explicit_choices_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let read = || load(&fs::read(&path).unwrap()).unwrap();
        let stored =
            || serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap();

        update_at(&path, "agent", "claude").unwrap();
        assert!(stored().get("shortcut").is_none());
        assert_eq!(read().shortcut, "⌘K");

        let mut legacy = serde_json::json!({
            "agent":"codex", "shortcut":"⇧⌘K", "newTabMode":"agent",
            "ollamaHost":"http://localhost:11434", "ollamaModel":"coder",
            "codexPath":"/opt/codex", "claudePath":""
        });
        fs::write(&path, legacy.to_string()).unwrap();
        assert_eq!(read().shortcut, "⌘K"); // effective at launch, before any settings change
        update_at(&path, "agent", "opencode").unwrap();
        assert_eq!(stored()["formatVersion"], 1);
        assert!(stored().get("shortcut").is_none());

        legacy["shortcut"] = "⌥⌘Y".into();
        fs::write(&path, legacy.to_string()).unwrap();
        assert_eq!(read().shortcut, "⌥⌘Y");
        update_at(&path, "agent", "claude").unwrap();
        assert_eq!(stored()["shortcut"], "⌥⌘Y");

        update_at(&path, "shortcut", "⇧⌘K").unwrap();
        update_at(&path, "newTabMode", "terminal").unwrap();
        assert_eq!(read().shortcut, "⇧⌘K");
        assert_eq!(stored()["shortcut"], "⇧⌘K");
    }
}
