use std::{
    env,
    path::{Path, PathBuf},
};

fn directory() -> Result<Option<PathBuf>, String> {
    let mut args = env::args_os()
        .skip(1)
        .filter(|a| !a.to_string_lossy().starts_with("-psn_"));
    let first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("key")) {
        minterm::key_command::run(&args.collect::<Vec<_>>())?;
        return Ok(None);
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!(
            "Minterm\n\nUsage: minterm [DIRECTORY]\n       minterm key set|remove openai|anthropic\n\nOpen a terminal in the given directory. Manage provider keys from the prompt or the Settings window."
        );
        return Ok(None);
    }
    if args.next().is_some() {
        return Err("Usage: minterm [DIRECTORY]".into());
    }
    let path = match first {
        Some(arg) => PathBuf::from(arg),
        None => {
            let pwd = env::current_dir().map_err(|e| e.to_string())?;
            if pwd == Path::new("/") {
                env::var_os("HOME").map(PathBuf::from).unwrap_or(pwd)
            } else {
                pwd
            }
        }
    };
    if !path.is_dir() {
        return Err(format!("Directory does not exist: {}", path.display()));
    }
    Ok(Some(path.canonicalize().map_err(|e| e.to_string())?))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(directory) = directory()? {
        minterm::launch(directory, true, |_, _| {})?;
    }
    Ok(())
}
