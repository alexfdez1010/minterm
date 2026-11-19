use crate::agent_config;
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
    #[serde(skip_deserializing)]
    pub agent_commands: Vec<AgentCommand>,
    #[serde(skip_deserializing)]
    pub agent_commands_error: Option<String>,
    pub shortcut: String,
    pub new_tab_mode: String,
    #[serde(skip_serializing)]
    codex_path: String,
    #[serde(skip_serializing)]
    claude_path: String,
    #[serde(skip_serializing)]
    opencode_path: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AgentCommand {
    pub name: String,
    pub agent: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shortcut: String,
}

fn built_in(agent: &str) -> bool {
    matches!(agent, "codex" | "claude" | "opencode")
}

impl AgentCommand {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let mut letters = self.name.bytes();
        if self.name.len() > 64
            || !letters.next().is_some_and(|b| b.is_ascii_alphabetic())
            || !letters.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err("Start the name with a letter. Use up to 64 letters, numbers, dots, dashes or underscores.".into());
        }
        if built_in(&self.name.to_ascii_lowercase()) && self.name != self.agent {
            return Err("Use a lowercase provider name only for its matching CLI.".into());
        }
        if !built_in(&self.agent) {
            return Err("Choose Codex, Claude Code or OpenCode.".into());
        }
        if self.command.trim().is_empty()
            || self.command.len() > 4096
            || self.command.contains('\0')
            || (self.command.chars().any(char::is_control)
                && crate::agent_cli::literal_executable(&self.command).is_none())
        {
            return Err("Enter a single-line shell command of up to 4096 bytes.".into());
        }
        Ok(())
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            agent: "codex".into(),
            agent_commands: Vec::new(),
            agent_commands_error: None,
            shortcut: "⌘K".into(),
            new_tab_mode: "terminal".into(),
            codex_path: String::new(),
            claude_path: String::new(),
            opencode_path: String::new(),
        }
    }
}

impl Preferences {
    pub fn agent_command(&self, name: &str) -> Option<&AgentCommand> {
        self.agent_commands.iter().find(|entry| entry.name == name)
    }

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
        "newTabMode" if matches!(value, "terminal" | "agent") => Ok(value),
        "shortcut" if accelerator(value).is_some() => Ok(value),
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

pub(crate) fn read_general() -> Preferences {
    path()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| load(&bytes).ok())
        .unwrap_or_default()
}

pub fn read() -> Preferences {
    let mut prefs = read_general();
    match agent_config::read(&prefs) {
        Ok(config) => {
            prefs.agent_commands = config.commands;
            prefs.agent = config.default;
        }
        Err(error) => prefs.agent_commands_error = Some(error),
    }
    prefs
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
    }
    saved.insert("formatVersion".into(), 1.into());
}

fn load(bytes: &[u8]) -> Result<Preferences, serde_json::Error> {
    let mut saved = serde_json::from_slice(bytes)?;
    migrate(&mut saved);
    serde_json::from_value(serde_json::Value::Object(saved))
}

pub fn update(field: &str, value: &str) -> Result<(), String> {
    if field == "agent" {
        return agent_config::set_default(value);
    }
    if field == "agentCommand" {
        #[derive(Deserialize)]
        struct Edit {
            previous: Option<AgentCommand>,
            replacement: Option<AgentCommand>,
        }
        let edit: Edit = serde_json::from_str(value).map_err(|_| "Invalid command edit.")?;
        agent_config::update(edit.previous.as_ref(), edit.replacement)?;
        return Ok(());
    }
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
    saved.insert(field.into(), value.into());
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
    fn command_settings_validate_shortcuts() {
        for shortcut in ["⌘J", "⌃⌥⇧⌘L", "⇧⌘K"] {
            assert!(accelerator(shortcut).is_some());
        }
        for shortcut in ["K", "⇧K", "⌘1", "⌘⇧K", "⌘Delete"] {
            assert!(accelerator(shortcut).is_none());
        }
    }

    #[test]
    fn custom_command_names_are_unambiguous_in_the_prompt() {
        let mut command = AgentCommand {
            name: "qwen-2.5".into(),
            agent: "opencode".into(),
            command: r#"opencode -m "local/qwen coder""#.into(),
            shortcut: String::new(),
        };
        assert!(command.validate().is_ok());
        for name in [
            "",
            "Codex",
            "Claude",
            "OPENCODE",
            "my cmd",
            "./file",
            "cmd;other",
        ] {
            command.name = name.into();
            assert!(command.validate().is_err(), "{name}");
        }
        command.name = "ocq".into();
        command.command = "opencode\nexit".into();
        assert!(command.validate().is_err());
        command.command = "ocq".into();
        command.agent = "missing".into();
        assert!(command.validate().is_err());
    }

    #[test]
    fn retired_ollama_backend_keeps_the_codex_executable_and_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let legacy = serde_json::json!({
            "agent": "codex-ollama", "shortcut": "⌥⌘Y", "newTabMode": "agent",
            "codexOllama": true, "ollamaHost": "http://localhost:11434", "ollamaModel": "coder:7b",
            "codexPath": "/tmp/my codex", "opencodePath": "/opt/Open Code/bin/client", "futureSetting": 9
        });
        fs::write(&path, legacy.to_string()).unwrap();
        update_at(&path, "newTabMode", "agent").unwrap();
        let saved = fs::read(path).unwrap();
        let prefs = load(&saved).unwrap();
        assert_eq!(prefs.agent, "codex");
        assert_eq!(prefs.shortcut, "⌥⌘Y");
        assert_eq!(prefs.new_tab_mode, "agent");
        assert_eq!(prefs.executable("codex"), Some(Path::new("/tmp/my codex")));
        assert_eq!(
            prefs.executable("opencode"),
            Some(Path::new("/opt/Open Code/bin/client"))
        );
        let snapshot = serde_json::to_value(&prefs).unwrap();
        for key in ["codexOllama", "ollamaHost", "ollamaModel"] {
            assert!(snapshot.get(key).is_none());
            assert!(validated_value(key, "ignored").is_err());
        }
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&saved).unwrap()["futureSetting"],
            9
        );
    }

    #[test]
    fn legacy_shortcuts_migrate_once_and_new_explicit_choices_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let read = || load(&fs::read(&path).unwrap()).unwrap();
        let stored =
            || serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap();

        update_at(&path, "newTabMode", "terminal").unwrap();
        assert!(stored().get("shortcut").is_none());
        assert_eq!(read().shortcut, "⌘K");

        let mut legacy = serde_json::json!({
            "agent":"codex", "shortcut":"⇧⌘K", "newTabMode":"agent",
            "codexPath":"/opt/codex", "claudePath":""
        });
        fs::write(&path, legacy.to_string()).unwrap();
        assert_eq!(read().shortcut, "⌘K"); // effective at launch, before any settings change
        update_at(&path, "newTabMode", "agent").unwrap();
        assert_eq!(stored()["formatVersion"], 1);
        assert!(stored().get("shortcut").is_none());

        legacy["shortcut"] = "⌥⌘Y".into();
        fs::write(&path, legacy.to_string()).unwrap();
        assert_eq!(read().shortcut, "⌥⌘Y");
        update_at(&path, "newTabMode", "terminal").unwrap();
        assert_eq!(stored()["shortcut"], "⌥⌘Y");

        update_at(&path, "shortcut", "⇧⌘K").unwrap();
        update_at(&path, "newTabMode", "terminal").unwrap();
        assert_eq!(read().shortcut, "⇧⌘K");
        assert_eq!(stored()["shortcut"], "⇧⌘K");
    }
}
