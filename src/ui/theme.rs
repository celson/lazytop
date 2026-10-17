use std::collections::BTreeMap;

use anyhow::bail;
use ratatui::style::Color;

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub border: Color,
    pub border_focus: Color,
    pub accent: Color,
    pub e_core: Color,
    pub p_core: Color,
    pub gpu: Color,
    pub ane: Color,
    pub media: Color,
    pub mem_wired: Color,
    pub mem_active: Color,
    pub mem_compressed: Color,
    pub mem_free: Color,
    pub rx: Color,
    pub tx: Color,
    pub read: Color,
    pub write: Color,
    pub ok: Color,
    pub warn: Color,
    pub crit: Color,
    pub bar_bg: Color,
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn rgb(hex: &str) -> Color {
    parse_hex(hex).expect("valid built-in color")
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: rgb("#16171b"),
            fg: rgb("#e6e6e6"),
            dim: rgb("#7d8188"),
            border: rgb("#2c2e34"),
            border_focus: rgb("#5b8def"),
            accent: rgb("#5b8def"),
            e_core: rgb("#d9b25f"),
            p_core: rgb("#6f8fd8"),
            gpu: rgb("#5fbf7f"),
            ane: rgb("#a07fd8"),
            media: rgb("#d98f4e"),
            mem_wired: rgb("#9b4f7a"),
            mem_active: rgb("#c779a2"),
            mem_compressed: rgb("#d9b8c8"),
            mem_free: rgb("#3a3c42"),
            rx: rgb("#5fbfa3"),
            tx: rgb("#e0913f"),
            read: rgb("#5fbfa3"),
            write: rgb("#e0913f"),
            ok: rgb("#5fbf7f"),
            warn: rgb("#e0b03f"),
            crit: rgb("#e05f5f"),
            bar_bg: rgb("#2c2e34"),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: rgb("#fafafa"),
            fg: rgb("#1d1f23"),
            dim: rgb("#6b6f76"),
            border: rgb("#d4d6db"),
            border_focus: rgb("#2f6fe0"),
            accent: rgb("#2f6fe0"),
            e_core: rgb("#a87b1c"),
            p_core: rgb("#2f5fbf"),
            gpu: rgb("#2f8f4f"),
            ane: rgb("#6f4fbf"),
            media: rgb("#b0601f"),
            mem_wired: rgb("#7a2f5a"),
            mem_active: rgb("#b0507f"),
            mem_compressed: rgb("#c99fb4"),
            mem_free: rgb("#e2e3e6"),
            rx: rgb("#1f8f73"),
            tx: rgb("#c0701f"),
            read: rgb("#1f8f73"),
            write: rgb("#c0701f"),
            ok: rgb("#2f8f4f"),
            warn: rgb("#b0801f"),
            crit: rgb("#c03f3f"),
            bar_bg: rgb("#e2e3e6"),
        }
    }

    pub fn mono() -> Self {
        let (fg, dim) = (Color::White, Color::Gray);
        Self {
            bg: Color::Reset,
            fg,
            dim,
            border: Color::DarkGray,
            border_focus: fg,
            accent: fg,
            e_core: fg,
            p_core: dim,
            gpu: fg,
            ane: fg,
            media: fg,
            mem_wired: fg,
            mem_active: dim,
            mem_compressed: Color::DarkGray,
            mem_free: Color::Black,
            rx: fg,
            tx: dim,
            read: fg,
            write: dim,
            ok: fg,
            warn: fg,
            crit: fg,
            bar_bg: Color::DarkGray,
        }
    }

    pub fn by_name(name: &str) -> Option<Theme> {
        match name {
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            "mono" => Some(Self::mono()),
            _ => None,
        }
    }

    pub fn with_overrides(mut self, colors: &BTreeMap<String, String>) -> anyhow::Result<Theme> {
        for (k, v) in colors {
            let Some(c) = parse_hex(v) else {
                bail!("colors.{k}: {v:?} is not #rrggbb")
            };
            let slot = match k.as_str() {
                "bg" => &mut self.bg,
                "fg" => &mut self.fg,
                "dim" => &mut self.dim,
                "border" => &mut self.border,
                "border_focus" => &mut self.border_focus,
                "accent" => &mut self.accent,
                "e_core" => &mut self.e_core,
                "p_core" => &mut self.p_core,
                "gpu" => &mut self.gpu,
                "ane" => &mut self.ane,
                "media" => &mut self.media,
                "mem_wired" => &mut self.mem_wired,
                "mem_active" => &mut self.mem_active,
                "mem_compressed" => &mut self.mem_compressed,
                "mem_free" => &mut self.mem_free,
                "rx" => &mut self.rx,
                "tx" => &mut self.tx,
                "read" => &mut self.read,
                "write" => &mut self.write,
                "ok" => &mut self.ok,
                "warn" => &mut self.warn,
                "crit" => &mut self.crit,
                "bar_bg" => &mut self.bar_bg,
                _ => bail!("unknown color key colors.{k}"),
            };
            *slot = c;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex() {
        assert_eq!(parse_hex("#5b8def"), Some(Color::Rgb(0x5b, 0x8d, 0xef)));
        assert_eq!(parse_hex("5b8def"), None);
        assert_eq!(parse_hex("#xyz123"), None);
    }

    #[test]
    fn overrides() {
        let mut m = BTreeMap::new();
        m.insert("accent".to_string(), "#010203".to_string());
        let t = Theme::dark().with_overrides(&m).unwrap();
        assert_eq!(t.accent, Color::Rgb(1, 2, 3));
        m.insert("nope".into(), "#000000".into());
        assert!(Theme::dark().with_overrides(&m).is_err());
        assert!(Theme::by_name("neon").is_none());
    }
}
