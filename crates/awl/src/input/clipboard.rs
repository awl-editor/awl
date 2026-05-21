pub fn get_clipboard() -> String {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default()
}

pub fn set_clipboard(text: &str) {
    let text = text.to_string();
    std::thread::spawn(move || {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(&text);
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });
}
