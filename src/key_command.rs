use crate::credentials::{self, Provider};
use std::ffi::{CStr, OsString};

const USAGE: &str = "Usage: minterm key set|remove openai|anthropic";
const RPP_REQUIRE_TTY: libc::c_int = 0x02; // macOS <readpassphrase.h>

unsafe extern "C" {
    fn readpassphrase(
        prompt: *const libc::c_char,
        buffer: *mut libc::c_char,
        size: usize,
        flags: libc::c_int,
    ) -> *mut libc::c_char;
}

pub fn run(args: &[OsString]) -> Result<(), String> {
    let [action, name] = args else {
        return Err(USAGE.into());
    };
    let (provider, label, prompt) = match name.to_str() {
        Some("openai") => (Provider::OpenAi, "OpenAI", c"OpenAI API key (hidden): "),
        Some("anthropic") => (
            Provider::Anthropic,
            "Anthropic",
            c"Anthropic API key (hidden): ",
        ),
        _ => return Err(USAGE.into()),
    };
    match action.to_str() {
        Some("set") => {
            prompt_and_save(provider, prompt)?;
            eprintln!("Saved the {label} API key.");
        }
        Some("remove") => {
            credentials::remove(provider)?;
            eprintln!("Cleared the saved {label} API key.");
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn prompt_and_save(provider: Provider, prompt: &CStr) -> Result<(), String> {
    // Allow a sentinel byte past the 4096-byte limit to detect truncation.
    let mut buffer = [0u8; 4098];
    // SAFETY: the prompt is NUL-terminated and the buffer is writable for size bytes.
    let read = unsafe {
        readpassphrase(
            prompt.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            RPP_REQUIRE_TTY,
        )
    };
    let result = if read.is_null() {
        Err("Could not read the API key. Run this command in an interactive terminal.".into())
    } else {
        CStr::from_bytes_until_nul(&buffer)
            .ok()
            .and_then(|s| s.to_str().ok())
            .filter(|s| s.len() <= 4096)
            .ok_or_else(|| "Enter a valid API key of up to 4096 bytes.".to_owned())
            .and_then(|key| credentials::save(provider, key))
    };
    // SAFETY: memset_s clears exactly this live buffer, including on read errors.
    unsafe { libc::memset_s(buffer.as_mut_ptr().cast(), buffer.len(), 0, buffer.len()) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_keys_in_arguments_without_echoing_them() {
        for args in [
            vec!["set", "openai", "fake-secret"],
            vec!["set", "unknown"],
            vec!["show", "anthropic"],
            vec!["set"],
        ] {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(run(&args).unwrap_err(), USAGE);
        }
    }
}
