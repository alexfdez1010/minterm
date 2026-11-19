use crate::credentials::{self, Provider, Source};
use std::ffi::c_char;

type Update = unsafe extern "C" fn(i32, i32, *const u8, usize, *mut c_char, usize) -> i32;

unsafe extern "C" {
    fn minterm_settings_show(update: Update);
    fn minterm_settings_close_if_key() -> bool;
}

/// Called on the AppKit main thread by the application's event loop.
pub fn show() {
    unsafe { minterm_settings_show(update) };
}

pub fn close_if_key() -> bool {
    unsafe { minterm_settings_close_if_key() }
}

/// Called by the panel's serial worker. Returns 1 if a saved key exists,
/// 0 if absent, and -1 for a recoverable error. Output contains UI feedback.
unsafe extern "C" fn update(
    action: i32,
    provider: i32,
    key: *const u8,
    length: usize,
    message: *mut c_char,
    capacity: usize,
) -> i32 {
    let provider = if provider == 0 {
        Provider::OpenAi
    } else {
        Provider::Anthropic
    };
    let label = if provider == Provider::OpenAi {
        "OpenAI"
    } else {
        "Anthropic"
    };
    let outcome = match action {
        0 => credentials::source(provider).map(|source| match source {
            Source::Saved => (1, "Saved on this Mac.".into()),
            Source::Missing => (0, "No API key configured.".into()),
            Source::Environment(name) => (0, format!("Using {name} from the environment.")),
        }),
        1 if !key.is_null() && length <= 4096 => {
            // SAFETY: NSString's contiguous UTF-8 bytes remain alive throughout the callback.
            let bytes = unsafe { std::slice::from_raw_parts(key, length) };
            std::str::from_utf8(bytes)
                .map_err(|_| "Enter a valid API key.".into())
                .and_then(|key| credentials::save(provider, key))
                .map(|()| (1, format!("{label} key saved.")))
        }
        2 => credentials::remove(provider).map(|()| (0, format!("Cleared the saved {label} key."))),
        _ => Err("Enter a valid API key of up to 4096 bytes.".into()),
    };
    let (state, text) = outcome.unwrap_or_else(|error| (-1, error));
    if !message.is_null() && capacity > 0 {
        let mut end = (capacity - 1).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        // SAFETY: the caller supplies a writable buffer of capacity bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), message.cast(), end);
            *message.add(end) = 0;
        }
    }
    state
}
