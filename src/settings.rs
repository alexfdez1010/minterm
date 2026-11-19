use crate::credentials::{self, Provider, Source};
use crate::{app::Message, preferences};
use std::{
    cell::OnceCell,
    ffi::{CStr, CString, c_char},
};
use tao::event_loop::EventLoopProxy;

type Update = unsafe extern "C" fn(i32, i32, *const u8, usize, *mut c_char, usize) -> i32;

unsafe extern "C" {
    fn minterm_settings_show(update: Update, prefs: *const c_char, change: Change);
    fn minterm_settings_close_if_key() -> bool;
}

type Change = unsafe extern "C" fn(*const c_char, *const c_char, *mut c_char, usize) -> bool;
thread_local! { static EVENTS: OnceCell<EventLoopProxy<Message>> = const { OnceCell::new() }; }

pub fn notify_changes(proxy: EventLoopProxy<Message>) {
    EVENTS.with(|events| {
        let _ = events.set(proxy);
    });
}

// The Settings window owns both UTF-8 strings throughout this synchronous call.
unsafe extern "C" fn change(
    field: *const c_char,
    value: *const c_char,
    error: *mut c_char,
    capacity: usize,
) -> bool {
    let field = unsafe { CStr::from_ptr(field) }.to_str();
    let value = unsafe { CStr::from_ptr(value) }.to_str();
    let result = field
        .and_then(|field| value.map(|value| (field, value)))
        .map_err(|e| e.to_string())
        .and_then(|(field, value)| preferences::update(field, value));
    if let Err(message) = result {
        write_feedback(&message, error, capacity);
        return false;
    }
    EVENTS.with(|events| {
        if let Some(proxy) = events.get() {
            let _ = proxy.send_event(Message::Preferences);
        }
    });
    true
}

/// Called on the AppKit main thread by the application's event loop.
pub fn show() {
    let json =
        CString::new(serde_json::to_string(&preferences::read()).expect("serializable settings"))
            .expect("JSON escapes nulls");
    unsafe { minterm_settings_show(update, json.as_ptr(), change) };
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
    write_feedback(&text, message, capacity);
    state
}

fn write_feedback(text: &str, message: *mut c_char, capacity: usize) {
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
}
