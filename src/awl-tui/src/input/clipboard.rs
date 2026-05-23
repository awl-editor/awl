use std::sync::{Mutex, OnceLock};

static INTERNAL: OnceLock<Mutex<String>> = OnceLock::new();

fn internal() -> &'static Mutex<String> {
    INTERNAL.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_clipboard() -> String {
    // Try the system clipboard first so that content copied from other
    // applications takes priority.  If arboard fails or returns empty
    // (which can happen on X11 when this process owns the selection and
    // the background keepalive thread is sleeping rather than pumping
    // events), fall back to the text we stored when set_clipboard was
    // last called.
    match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(s) if !s.is_empty() => s,
        _ => internal().lock().unwrap().clone(),
    }
}

pub fn set_clipboard(text: &str) {
    *internal().lock().unwrap() = text.to_string();
    let text = text.to_string();
    std::thread::spawn(move || {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(&text);
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });
}
