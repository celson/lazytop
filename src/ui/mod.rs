pub mod fmt;
pub mod theme;
pub mod widgets;

#[cfg(test)]
pub mod test_util {
    use ratatui::{Frame, Terminal, backend::TestBackend};

    /// Renders into a TestBackend and returns the screen as lines joined by '\n'.
    pub fn render(w: u16, h: u16, f: impl FnOnce(&mut Frame)) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(f).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
