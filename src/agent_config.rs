use crate::preferences::{self, AgentCommand, Preferences};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

const STARTER: &str = include_str!("../config/agents.toml");

pub struct AgentConfig {
    pub commands: Vec<AgentCommand>,
    pub default: String,
}

pub fn path() -> Result<PathBuf, String> {
    let root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::home_dir().map(|home| home.join(".config")))
        .ok_or("Could not locate the configuration directory.")?;
    Ok(root.join("minterm/agents.toml"))
}

fn with_path(path: &Path, error: impl std::fmt::Display) -> String {
    format!("{}: {error}", path.display())
}

// Render the seed with any pre-TOML default or executable overrides. Installation
// never clobbers a config created concurrently by an editor or another instance.
fn ensure_at(path: &Path, legacy: &Preferences) -> Result<(), String> {
    let err = |e| with_path(path, e);
    if path.try_exists().map_err(err)? {
        return Ok(());
    }
    let mut seed = document(STARTER)?;
    let commands = seed["commands"].as_array_of_tables_mut().unwrap();
    for entry in commands.iter_mut() {
        apply_legacy_path(entry, legacy)?;
    }
    if commands
        .iter()
        .any(|entry| entry["name"].as_str() == Some(&legacy.agent))
    {
        set_string(seed.as_table_mut(), "default", &legacy.agent);
    }
    config(&seed)?;
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).map_err(err)?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(err)?;
    file.write_all(seed.to_string().as_bytes()).map_err(err)?;
    match file.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(err(e.error)),
    }
}

pub fn ensure_file() -> Result<PathBuf, String> {
    let path = path()?;
    ensure_at(&path, &preferences::read_general())?;
    Ok(path)
}

fn apply_legacy_path(row: &mut Table, legacy: &Preferences) -> Result<(), String> {
    let name = row["name"].as_str().unwrap();
    if row["agent"].as_str() == Some(name)
        && let Some(path) = legacy.executable(name)
    {
        if !path.is_absolute() {
            return Err(format!("Expected an absolute executable path for {name}."));
        }
        set_string(row, "command", &shell_words::quote(path.to_str().unwrap()));
    }
    Ok(())
}

// Normalize both TOML table-list notations for the reader and the Settings editor.
fn document(text: &str) -> Result<DocumentMut, String> {
    let mut doc = text.parse::<DocumentMut>().map_err(|e| {
        let line = e.span().map(|s| {
            1 + text.as_bytes()[..s.start]
                .iter()
                .filter(|c| **c == b'\n')
                .count()
        });
        match line {
            Some(n) => format!("Invalid TOML near line {n}. {}", e.message()),
            None => format!("Invalid TOML. {}", e.message()),
        }
    })?;
    if let Some(item) = doc.get_mut("commands")
        && !item.is_array_of_tables()
    {
        let array = if item.as_array().is_some_and(|a| a.is_empty()) {
            ArrayOfTables::new()
        } else {
            std::mem::take(item)
                .into_array_of_tables()
                .map_err(|_| "Use [[commands]] for each agent command.")?
        };
        *item = Item::ArrayOfTables(array);
    }
    if let Some(version) = doc.get("version")
        && !matches!(version.as_integer(), Some(1 | 2))
    {
        return Err("Unsupported agent configuration version.".into());
    }
    Ok(doc)
}

fn config(doc: &DocumentMut) -> Result<AgentConfig, String> {
    let mut commands = Vec::new();
    if let Some(rows) = doc.get("commands").and_then(Item::as_array_of_tables) {
        let mut names = HashSet::new();
        let mut shortcut_owners = HashMap::new();
        for (index, entry) in rows.iter().enumerate() {
            let field = |key| {
                entry
                    .get(key)
                    .and_then(Item::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("Command {} needs a text field named {key}.", index + 1))
            };
            let command = AgentCommand {
                name: field("name")?,
                agent: field("agent")?,
                command: field("command")?,
                shortcut: if entry.contains_key("shortcut") {
                    canonical_shortcut(&field("shortcut")?)?
                } else {
                    String::new()
                },
            };
            command.validate()?;
            if !names.insert(command.name.clone()) {
                return Err(format!("The name {} is used more than once.", command.name));
            }
            // Legacy Enter shortcuts also accept Ctrl with the same letter.
            if let Some((_, letter)) = command.shortcut.split_once('+')
                && let Some(other) = shortcut_owners.insert(letter.to_owned(), command.name.clone())
            {
                return Err(format!(
                    "Shortcut letter {letter} is assigned to both {other} and {}. Choose a different letter.",
                    command.name
                ));
            }
            commands.push(command);
        }
    }
    let first = commands.first().ok_or("Keep at least one agent profile.")?;
    let default = match doc.get("default") {
        Some(item) => {
            let name = item.as_str().ok_or("The default must be a profile name.")?;
            if !commands.iter().any(|c| c.name == name) {
                return Err(format!("Default profile {name} was not found."));
            }
            name.to_owned()
        }
        None => first.name.clone(),
    };
    Ok(AgentConfig { default, commands })
}

// Earlier files listed only custom commands. Upgrade the three implicit providers
// once; a versioned file may intentionally omit any of them.
fn upgrade(doc: &mut DocumentMut, legacy: &Preferences) -> Result<(), String> {
    if doc.get("version").and_then(Item::as_integer) == Some(2) {
        return Ok(());
    }
    let rows = doc
        .entry("commands")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .unwrap();
    let seed = document(STARTER)?;
    for (index, stock) in seed["commands"]
        .as_array_of_tables()
        .unwrap()
        .iter()
        .filter(|row| row["name"].as_str() == row["agent"].as_str())
        .enumerate()
    {
        let mut row = stock.clone();
        // Serialize new providers before the original rows regardless of their seed positions.
        row.set_position(Some(0));
        apply_legacy_path(&mut row, legacy)?;
        rows.insert(index, row);
    }
    let default = if rows
        .iter()
        .any(|entry| entry.get("name").and_then(Item::as_str) == Some(&legacy.agent))
    {
        legacy.agent.as_str()
    } else {
        "codex"
    };
    if !doc.contains_key("default") {
        set_string(doc.as_table_mut(), "default", default);
    }
    doc["version"] = toml_edit::value(2);
    Ok(())
}

fn canonical_shortcut(shortcut: &str) -> Result<String, String> {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        return Ok(String::new());
    }
    if let Some((modifier, key)) = shortcut.split_once('+') {
        let key = key.trim();
        let modifier = match modifier.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some("Ctrl"),
            "enter" => Some("Enter"),
            _ => None,
        };
        if let Some(modifier) = modifier
            && key.len() == 1
            && key.as_bytes()[0].is_ascii_alphabetic()
        {
            return Ok(format!("{modifier}+{}", key.to_ascii_uppercase()));
        }
    }
    Err("Use Ctrl or Enter with one letter, such as Ctrl+L.".into())
}

pub fn read(legacy: &Preferences) -> Result<AgentConfig, String> {
    let path = path()?;
    read_at(&path, legacy)
}

fn read_at(path: &Path, legacy: &Preferences) -> Result<AgentConfig, String> {
    ensure_at(path, legacy)?;
    let text = fs::read_to_string(path).map_err(|e| with_path(path, e))?;
    let doc = document(&text).map_err(|e| with_path(path, e))?;
    if doc.get("version").and_then(Item::as_integer) == Some(2) {
        return config(&doc).map_err(|e| with_path(path, e));
    }
    edit_at(path, legacy, |_, _| Ok(()))
}

fn set_string(table: &mut Table, key: &str, text: &str) {
    if table.get(key).and_then(Item::as_str) == Some(text) {
        return;
    }
    let mut value = Value::from(text);
    if let Some(old) = table.get(key).and_then(Item::as_value) {
        *value.decor_mut() = old.decor().clone();
    }
    table[key] = Item::Value(value);
}

fn edit_at(
    path: &Path,
    legacy: &Preferences,
    edit: impl FnOnce(&mut DocumentMut, &AgentConfig) -> Result<(), String>,
) -> Result<AgentConfig, String> {
    // Keep dotfile-manager symlinks intact. Serialize edits from Minterm windows
    // against the same underlying file and keep unrelated TOML and comments.
    let path = fs::canonicalize(path).map_err(|e| with_path(path, e))?;
    let err = |e: io::Error| with_path(&path, e);
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("toml.lock"))
        .map_err(err)?;
    lock.lock().map_err(err)?;
    let text = fs::read_to_string(&path).map_err(err)?;
    let mut doc = document(&text).map_err(|e| with_path(&path, e))?;
    upgrade(&mut doc, legacy).map_err(|e| with_path(&path, e))?;
    let old = config(&doc).map_err(|e| with_path(&path, e))?;
    edit(&mut doc, &old)?;
    let content = doc.to_string();
    let result = config(&doc)?;
    if content != text {
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(err)?;
        temp.write_all(content.as_bytes()).map_err(err)?;
        temp.flush().map_err(err)?;
        if fs::read_to_string(&path).map_err(err)? != text {
            return Err(
                "The configuration changed on disk. Reopen the editor and try again.".into(),
            );
        }
        temp.persist(&path).map_err(|e| err(e.error))?;
    }
    Ok(result)
}

pub fn set_default(name: &str) -> Result<(), String> {
    edit_at(&ensure_file()?, &preferences::read_general(), |doc, _| {
        set_string(doc.as_table_mut(), "default", name);
        Ok(())
    })
    .map(|_| ())
}

fn update_entry(
    doc: &mut DocumentMut,
    config: &AgentConfig,
    previous: Option<&AgentCommand>,
    replacement: Option<&AgentCommand>,
) -> Result<(), String> {
    let index = previous
        .map(|old| {
            config
                .commands
                .iter()
                .position(|c| c == old)
                .ok_or("This command changed on disk. Reopen the editor and try again.")
        })
        .transpose()?;
    if let Some(old) = previous
        && config.default == old.name
    {
        let next = match replacement {
            Some(command) => &command.name,
            None => {
                let next = config
                    .commands
                    .iter()
                    .filter(|c| c.name != old.name)
                    .min_by_key(|c| {
                        if c.name == old.agent {
                            0
                        } else if c.agent == old.agent {
                            1
                        } else if c.name == "codex" {
                            2
                        } else {
                            3
                        }
                    })
                    .ok_or("Keep at least one agent profile.")?;
                &next.name
            }
        };
        set_string(doc.as_table_mut(), "default", next);
    }
    let rows = doc
        .entry("commands")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .unwrap();
    if let Some(command) = replacement {
        command.validate()?;
        let row = match index {
            Some(i) => rows.get_mut(i).unwrap(),
            None => {
                rows.push(Table::new());
                rows.get_mut(rows.len() - 1).unwrap()
            }
        };
        for (key, text) in [
            ("name", &command.name),
            ("agent", &command.agent),
            ("command", &command.command),
        ] {
            set_string(row, key, text);
        }
        if command.shortcut.is_empty() {
            row.remove("shortcut");
        } else {
            set_string(row, "shortcut", &command.shortcut);
        }
    } else if let Some(index) = index {
        rows.remove(index);
    } else {
        return Err("Choose a command to edit.".into());
    }
    Ok(())
}

pub fn update(
    previous: Option<&AgentCommand>,
    mut replacement: Option<AgentCommand>,
) -> Result<AgentConfig, String> {
    if let Some(command) = &mut replacement {
        command.shortcut = canonical_shortcut(&command.shortcut)?;
    }
    edit_at(
        &ensure_file()?,
        &preferences::read_general(),
        |doc, config| update_entry(doc, config, previous, replacement.as_ref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_prefs(agent: &str, codex_path: &str) -> Preferences {
        serde_json::from_value(serde_json::json!({ "agent": agent, "codexPath": codex_path }))
            .unwrap()
    }

    #[test]
    fn starter_lists_every_profile_and_the_selected_default() {
        let config = config(&document(STARTER).unwrap()).unwrap();
        assert_eq!(config.default, "codex");
        let names: Vec<_> = config
            .commands
            .iter()
            .map(|c| (&*c.name, &*c.shortcut))
            .collect();
        assert_eq!(
            names,
            [
                ("codex", ""),
                ("claude", ""),
                ("opencode", ""),
                ("ocq", ""),
                ("sol-light", "Ctrl+L"),
                ("sol-ultra", "Ctrl+U")
            ]
        );
        assert_eq!(config.commands[3].command, "ocq --");
    }

    #[test]
    fn shortcut_letters_work_across_current_and_legacy_profiles() {
        for (input, expected) in [
            ("  control + l ", "Ctrl+L"),
            ("CtRL+u", "Ctrl+U"),
            (" enter + l", "Enter+L"),
            (" ", ""),
        ] {
            assert_eq!(canonical_shortcut(input).unwrap(), expected);
        }
        for invalid in ["Ctrl+", "Ctrl+1", "Ctrl+é", "Ctrl+Shift+L", "Alt+L"] {
            assert!(canonical_shortcut(invalid).is_err());
        }

        let collision = STARTER.replace("Ctrl+L", "Enter+U");
        let error = config(&document(&collision).unwrap()).err().unwrap();
        assert!(
            error.contains("letter U")
                && error.contains("sol-light")
                && error.contains("sol-ultra")
        );
    }

    #[test]
    fn migration_preserves_legacy_files_and_literal_executables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let program = dir.path().join("my 'bin' \\$(touch unexpected);$HOME");
        fs::copy("/bin/echo", &program).unwrap();
        let legacy = legacy_prefs("claude", program.to_str().unwrap());
        // Both migrated and first-time TOML users may have a saved executable.
        for existing in [
            None,
            Some(
                "[[commands]] # team preset\nname='team'\nagent='claude'\ncommand='my-claude' # keep\nshortcut='enter + t'\n",
            ),
        ] {
            if let Some(text) = existing {
                fs::write(&path, text).unwrap();
            }
            let config = read_at(&path, &legacy).unwrap();
            assert_eq!(config.default, "claude");
            let codex = config.commands.iter().find(|p| p.name == "codex").unwrap();
            let executable = crate::agent_cli::literal_executable(&codex.command).unwrap();
            let cli = crate::agent_cli::resolve(&codex.agent, Some(&executable)).unwrap();
            assert_eq!(cli.program, program);
            assert!(cli.args.is_empty());
            let saved = fs::read_to_string(&path).unwrap();
            assert!(saved.contains("version = 2"));
            if existing.is_some() {
                assert_eq!(config.commands.len(), 4);
                assert!(saved.contains("# team preset") && saved.contains("# keep"));
                assert_eq!(
                    config
                        .commands
                        .iter()
                        .find(|p| p.name == "team")
                        .unwrap()
                        .shortcut,
                    "Enter+T"
                );
            }
            let second = read_at(&path, &legacy).unwrap();
            assert_eq!(second.commands, config.commands);
            assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        }
    }

    #[test]
    fn edits_preserve_comments_and_fallbacks_without_restoring_deleted_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let link = dir.path().join("linked.toml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let legacy = Preferences::default();
        fs::write(&path, "default = 'local' # chosen\n[[commands]]\nname='local'\nagent='opencode'\ncommand='ocq --' # keep\n").unwrap();
        let c = read_at(&link, &legacy).unwrap();
        assert_eq!(c.default, "local");
        let old = c
            .commands
            .iter()
            .find(|p| p.name == "local")
            .unwrap()
            .clone();
        let mut new = old.clone();
        new.name = "quick".into();
        let edit = |old: &AgentCommand, new: Option<&AgentCommand>| {
            edit_at(&link, &legacy, |doc, config| {
                update_entry(doc, config, Some(old), new)
            })
        };
        assert_eq!(edit(&old, Some(&new)).unwrap().default, "quick");
        assert!(link.is_symlink());
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# chosen") && saved.contains("# keep"));
        assert!(edit(&old, None).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        let result = edit(&new, None).unwrap();
        assert_eq!(result.default, "opencode");
        let opencode = result
            .commands
            .iter()
            .find(|p| p.name == "opencode")
            .unwrap();
        let mut tweaked = opencode.clone();
        tweaked.command = "opencode --standalone".into();
        tweaked.shortcut = "Enter+O".into();
        assert!(
            edit(opencode, Some(&tweaked))
                .unwrap()
                .commands
                .contains(&tweaked)
        );
        let mut unbound = tweaked.clone();
        unbound.shortcut.clear();
        let updated = edit(&tweaked, Some(&unbound)).unwrap();
        assert!(updated.commands.contains(&unbound));
        assert_eq!(edit(&unbound, None).unwrap().default, "codex");
        let reloaded = read_at(&link, &legacy).unwrap();
        assert!(!reloaded.commands.iter().any(|p| p.agent == "opencode"));
        let codex = reloaded
            .commands
            .iter()
            .find(|p| p.name == "codex")
            .unwrap();
        let remaining = edit(codex, None).unwrap();
        assert_eq!(remaining.default, "claude");
        assert!(edit(&remaining.commands[0], None).is_err());
    }

    #[test]
    fn deleting_the_default_prefers_another_profile_for_its_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let legacy = Preferences::default();
        fs::write(
            &path,
            STARTER.replace("default = \"codex\"", "default = \"opencode\""),
        )
        .unwrap();
        let config = read_at(&path, &legacy).unwrap();
        let old = config
            .commands
            .iter()
            .find(|p| p.name == "opencode")
            .unwrap();
        let c = edit_at(&path, &legacy, |doc, config| {
            update_entry(doc, config, Some(old), None)
        })
        .unwrap();
        assert_eq!(c.default, "ocq");
    }

    #[test]
    fn invalid_external_edits_are_reported_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let legacy = Preferences::default();
        for bad in [
            STARTER.replace("Ctrl+U", "enter + l"),
            STARTER.replace("sol-ultra", "sol-light"),
            "default='missing'".into(),
            "commands = [".into(),
            "commands=['oops']".into(),
            "version=2\ncommands=[]".into(),
            "version=99".into(),
            "[[commands]]\nname='codex'\nagent='claude'\ncommand='claude'".into(),
        ] {
            fs::write(&path, &bad).unwrap();
            assert!(read_at(&path, &legacy).is_err());
            assert!(edit_at(&path, &legacy, |_, _| Ok(())).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), bad);
        }
        fs::write(
            &path,
            "commands = [{name='fast',agent='codex',command='codex',shortcut=' enter + l '}]",
        )
        .unwrap();
        let c = read_at(&path, &legacy).unwrap();
        assert_eq!(
            c.commands
                .iter()
                .find(|p| p.name == "fast")
                .unwrap()
                .shortcut,
            "Enter+L"
        );
        fs::write(&path, "commands = []").unwrap();
        assert_eq!(read_at(&path, &legacy).unwrap().commands.len(), 3);
        let file = "[[commands]]\nname='local'\nagent='codex'\ncommand='codex'\n";
        fs::write(&path, format!("{file}{}", file.replace("local", "Local"))).unwrap();
        assert_eq!(read_at(&path, &legacy).unwrap().commands.len(), 5);
    }
}
