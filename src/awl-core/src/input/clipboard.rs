use std::sync::{Mutex, OnceLock};

static INTERNAL: OnceLock<Mutex<String>> = OnceLock::new();

fn internal() -> &'static Mutex<String> {
    INTERNAL.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_clipboard() -> String {
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
