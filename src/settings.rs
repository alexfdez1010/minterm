use crate::{app::Message, preferences};
use std::{
    cell::OnceCell,
    ffi::{CStr, CString, c_char},
};
use tao::event_loop::EventLoopProxy;

unsafe extern "C" {
    fn minterm_settings_show(prefs: *const c_char, change: Change);
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
    unsafe { minterm_settings_show(json.as_ptr(), change) };
}

pub fn close_if_key() -> bool {
    unsafe { minterm_settings_close_if_key() }
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
