use crate::app::App;

pub fn gutter_width(app: &App) -> u16 {
    let lines = app.current().map(|b| b.line_count()).unwrap_or(1).max(1);
    let digits = lines.ilog10() as u16 + 1;
    digits.max(3) + 2 // +1 diff indicator, +1 trailing space
}
