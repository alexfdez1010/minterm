use crate::preferences::AgentCommand;
use std::{
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

const STARTER: &str = include_str!("../config/agents.toml");

pub struct AgentConfig {
    pub commands: Vec<AgentCommand>,
    pub default: Option<String>,
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

fn ensure_at(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(STARTER.as_bytes())?;
    match file.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.error),
    }
}

pub fn ensure_file() -> Result<PathBuf, String> {
    let path = path()?;
    if !path.try_exists().map_err(|e| with_path(&path, e))? {
        ensure_at(&path).map_err(|e| with_path(&path, e))?;
    }
    Ok(path)
}

// Accept both TOML's [[commands]] spelling and arrays of inline tables.
fn parse(text: &str) -> Result<(DocumentMut, AgentConfig), String> {
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
    let default = doc
        .get("default")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("The default must be an agent or command name.")
        })
        .transpose()?;
    let mut commands = Vec::new();
    if let Some(item) = doc.get_mut("commands") {
        if !item.is_array_of_tables() {
            let array = if item.as_array().is_some_and(|a| a.is_empty()) {
                ArrayOfTables::new()
            } else {
                std::mem::take(item)
                    .into_array_of_tables()
                    .map_err(|_| "Use [[commands]] for each agent command.")?
            };
            *item = Item::ArrayOfTables(array);
        }
        let mut names = HashSet::new();
        let mut shortcuts = HashSet::new();
        for (index, entry) in item.as_array_of_tables().unwrap().iter().enumerate() {
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
            if !command.shortcut.is_empty()
                && !shortcuts.insert(command.shortcut.to_ascii_uppercase())
            {
                return Err(format!(
                    "The shortcut {} is used more than once.",
                    command.shortcut
                ));
            }
            commands.push(command);
        }
    }
    if let Some(ref name) = default
        && !matches!(name.as_str(), "codex" | "claude" | "opencode")
        && !commands.iter().any(|c| c.name == *name)
    {
        return Err(format!("Default command {name} was not found."));
    }
    Ok((doc, AgentConfig { default, commands }))
}

fn canonical_shortcut(shortcut: &str) -> Result<String, String> {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        return Ok(String::new());
    }
    if let Some((enter, key)) = shortcut.split_once('+') {
        let key = key.trim();
        if enter.trim().eq_ignore_ascii_case("enter")
            && key.len() == 1
            && key.as_bytes()[0].is_ascii_alphabetic()
        {
            return Ok(format!("Enter+{}", key.to_ascii_uppercase()));
        }
    }
    Err("Use Enter and one letter, such as Enter+L.".into())
}

pub fn read() -> Result<AgentConfig, String> {
    let path = ensure_file()?;
    let text = fs::read_to_string(&path).map_err(|e| with_path(&path, e))?;
    parse(&text)
        .map(|(_, config)| config)
        .map_err(|e| with_path(&path, e))
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
    let (mut doc, config) = parse(&text).map_err(|e| with_path(&path, e))?;
    edit(&mut doc, &config)?;
    let content = doc.to_string();
    let (_, result) = parse(&content)?;
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
    edit_at(&ensure_file()?, |doc, _| {
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
        && config.default.as_deref() == Some(old.name.as_str())
    {
        set_string(
            doc.as_table_mut(),
            "default",
            replacement.map_or(&old.agent, |c| &c.name),
        );
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
    edit_at(&ensure_file()?, |doc, config| {
        update_entry(doc, config, previous, replacement.as_ref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_commands_use_the_configured_wrappers_and_efforts() {
        let (_, config) = parse(STARTER).unwrap();
        assert!(config.default.is_none());
        let names: Vec<_> = config
            .commands
            .iter()
            .map(|c| (&*c.name, &*c.shortcut))
            .collect();
        assert_eq!(
            names,
            [
                ("ocq", ""),
                ("sol-light", "Enter+L"),
                ("sol-ultra", "Enter+U")
            ]
        );
        assert_eq!(config.commands[0].command, "ocq --");
    }

    #[test]
    fn edits_keep_other_commands_comments_and_the_default_in_sync() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let link = dir.path().join("linked.toml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let original = format!("default = 'sol-light' # chosen\n{STARTER}");
        fs::write(&path, &original).unwrap();
        let old = parse(&original).unwrap().1.commands.remove(1);
        let mut new = old.clone();
        new.name = "quick".into();
        let result = edit_at(&link, |doc, config| {
            update_entry(doc, config, Some(&old), Some(&new))
        })
        .unwrap();
        assert_eq!(result.default.as_deref(), Some("quick"));
        assert!(link.is_symlink());
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# chosen") && saved.contains("# The shell alias"));
        assert!(
            edit_at(&path, |doc, config| update_entry(
                doc,
                config,
                Some(&old),
                None
            ))
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        let result = edit_at(&path, |doc, config| {
            update_entry(doc, config, Some(&new), None)
        })
        .unwrap();
        assert_eq!(result.default.as_deref(), Some("codex"));
        assert_eq!(result.commands.len(), 2);
    }

    #[test]
    fn invalid_external_edits_are_reported_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.toml");
        let dupe = STARTER.replace("Enter+U", "enter + l");
        assert!(parse(&dupe).err().unwrap().contains("more than once"));
        for invalid in [
            dupe.as_str(),
            "default = 'missing'",
            "commands = [",
            "commands = ['oops']",
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(edit_at(&path, |_, _| Ok(())).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
        let file =
            "commands = [{ name='fast', agent='codex', command='codex', shortcut=' enter + l ' }]";
        assert_eq!(parse(file).unwrap().1.commands[0].shortcut, "Enter+L");
        fs::write(&path, "commands = []").unwrap();
        ensure_at(&path).unwrap();
        assert!(
            parse(&fs::read_to_string(path).unwrap())
                .unwrap()
                .1
                .commands
                .is_empty()
        );
    }
}
