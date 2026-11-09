//! Native palette handling. Custom JSON reads documented color tokens, never executes CSS.
use egui::{Color32, FontFamily, FontId};
use feishin_core::preferences::Appearance;
use std::collections::BTreeMap;

pub const THEMES: &[(&str, &str)] = &[
    ("defaultDark", "Default Dark"),
    ("defaultLight", "Default Light"),
    ("ayuDark", "Ayu Dark"),
    ("ayuLight", "Ayu Light"),
    ("catppuccinLatte", "Catppuccin Latte"),
    ("catppuccinMocha", "Catppuccin Mocha"),
    ("dracula", "Dracula"),
    ("everforestDark", "Everforest Dark"),
    ("everforestLight", "Everforest Light"),
    ("githubDark", "GitHub Dark"),
    ("githubLight", "GitHub Light"),
    ("glassyDark", "Glassy Dark"),
    ("gruvboxDark", "Gruvbox Dark"),
    ("gruvboxLight", "Gruvbox Light"),
    ("highContrastDark", "High Contrast Dark"),
    ("highContrastLight", "High Contrast Light"),
    ("materialDark", "Material Dark"),
    ("materialLight", "Material Light"),
    ("monokai", "Monokai"),
    ("nightOwl", "Night Owl"),
    ("nord", "Nord"),
    ("oneDark", "One Dark"),
    ("rosePine", "Rosé Pine"),
    ("rosePineDawn", "Rosé Pine Dawn"),
    ("rosePineMoon", "Rosé Pine Moon"),
    ("shadesOfPurple", "Shades of Purple"),
    ("solarizedDark", "Solarized Dark"),
    ("solarizedLight", "Solarized Light"),
    ("tokyoNight", "Tokyo Night"),
    ("vscodeDarkPlus", "VS Code Dark+"),
    ("vscodeLightPlus", "VS Code Light+"),
    ("zenburn", "Zenburn"),
    ("custom", "Custom palette"),
];
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color32,
    pub side: Color32,
    pub surface: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub light: bool,
}
impl Default for Palette {
    fn default() -> Self {
        Self::from_appearance(&Appearance::default(), egui::Theme::Dark)
    }
}
impl Palette {
    pub fn from_appearance(a: &Appearance, system: egui::Theme) -> Self {
        let id = if a.follow_system_theme {
            if system == egui::Theme::Light {
                "defaultLight"
            } else {
                "defaultDark"
            }
        } else {
            a.theme.as_str()
        };
        let (bg, surface, text, muted, light) = match id {
            "defaultLight" => (0xfafafa, 0xf0f0f0, 0x202020, 0x707070, true),
            "ayuDark" => (0x0b0e14, 0x11151c, 0xbfbdb6, 0x636a72, false),
            "ayuLight" => (0xf8f9fa, 0xebedf0, 0x5c6166, 0x787b80, true),
            "catppuccinLatte" => (0xeff1f5, 0xe6e9ef, 0x4c4f69, 0x7c7f93, true),
            "catppuccinMocha" => (0x1e1e2e, 0x313244, 0xcdd6f4, 0x9399b2, false),
            "dracula" => (0x282a36, 0x343746, 0xf8f8f2, 0x9597a5, false),
            "nord" => (0x2e3440, 0x3b4252, 0xeceff4, 0xa6b0c2, false),
            "everforestDark" => (0x2d353b, 0x343f44, 0xd3c6aa, 0x859289, false),
            "everforestLight" => (0xfdf6e3, 0xefebd4, 0x5c6a72, 0x939f91, true),
            "githubDark" => (0x0d1117, 0x161b22, 0xc9d1d9, 0x8b949e, false),
            "githubLight" => (0xffffff, 0xf6f8fa, 0x24292f, 0x57606a, true),
            "gruvboxDark" => (0x282828, 0x3c3836, 0xebdbb2, 0xa89984, false),
            "gruvboxLight" => (0xfbf1c7, 0xebdbb2, 0x3c3836, 0x7c6f64, true),
            "highContrastDark" => (0x000000, 0x101010, 0xffffff, 0xcfcfcf, false),
            "highContrastLight" => (0xffffff, 0xeeeeee, 0x000000, 0x333333, true),
            "materialDark" => (0x263238, 0x37474f, 0xeeffff, 0xb0bec5, false),
            "materialLight" => (0xfafafa, 0xeeeeee, 0x263238, 0x607d8b, true),
            "monokai" => (0x272822, 0x3e3d32, 0xf8f8f2, 0xa6a697, false),
            "nightOwl" => (0x011627, 0x0b2942, 0xd6deeb, 0x7e9bae, false),
            "oneDark" => (0x282c34, 0x353b45, 0xabb2bf, 0x7f848e, false),
            "rosePine" => (0x191724, 0x26233a, 0xe0def4, 0x908caa, false),
            "rosePineDawn" => (0xfaf4ed, 0xfffaf3, 0x575279, 0x797593, true),
            "rosePineMoon" => (0x232136, 0x2a273f, 0xe0def4, 0x908caa, false),
            "shadesOfPurple" => (0x2d2b55, 0x3d3b65, 0xffffff, 0xb3ace0, false),
            "solarizedDark" => (0x002b36, 0x073642, 0x93a1a1, 0x839496, false),
            "solarizedLight" => (0xfdf6e3, 0xeee8d5, 0x586e75, 0x839496, true),
            "tokyoNight" => (0x1a1b26, 0x24283b, 0xc0caf5, 0x9aa5ce, false),
            "vscodeDarkPlus" => (0x1e1e1e, 0x252526, 0xd4d4d4, 0x969696, false),
            "vscodeLightPlus" => (0xffffff, 0xf3f3f3, 0x333333, 0x616161, true),
            "zenburn" => (0x3f3f3f, 0x4f4f4f, 0xdcdccc, 0x9f9f8f, false),
            _ => (0x0a0a0a, 0x141414, 0xe1e1e1, 0x929292, false),
        };
        let c = |n: u32| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
        let mut p = Self {
            bg: c(bg),
            side: if light {
                c(surface)
            } else {
                c(bg).gamma_multiply(0.82)
            },
            surface: c(surface),
            line: if light {
                Color32::from_gray(218)
            } else {
                c(surface).gamma_multiply(1.4)
            },
            text: c(text),
            muted: c(muted),
            accent: Color32::from_rgb(a.accent_rgb[0], a.accent_rgb[1], a.accent_rgb[2]),
            light,
        };
        for (name, rgb) in &a.custom_colors {
            let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            match name.as_str() {
                "background" => p.bg = color,
                "background-alternate" => p.side = color,
                "surface" => p.surface = color,
                "foreground" | "surface-foreground" => p.text = color,
                "foreground-muted" => p.muted = color,
                "primary" => p.accent = color,
                _ => {}
            }
        }
        p
    }
}
pub fn apply(ctx: &egui::Context, a: &Appearance, p: Palette) {
    let mode = if p.light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    };
    ctx.set_theme(if a.follow_system_theme {
        egui::ThemePreference::System
    } else {
        mode.into()
    });
    let original = ctx.style_of(mode);
    let mut style = (*original).clone();
    style.visuals = if p.light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    style.visuals.override_text_color = Some(p.text);
    style.visuals.panel_fill = p.bg;
    style.visuals.window_fill = p.bg;
    style.visuals.extreme_bg_color = p.surface;
    style.visuals.faint_bg_color = p.surface;
    style.visuals.selection.bg_fill = p.accent;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.line);
    style.visuals.widgets.inactive.bg_fill = p.surface;
    style.visuals.widgets.inactive.weak_bg_fill = p.surface;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.hovered.bg_fill = if p.light {
        p.surface.gamma_multiply(0.94)
    } else {
        p.surface.gamma_multiply(1.6)
    };
    style.visuals.widgets.hovered.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    let family = if a.font_family == "Poppins" {
        "Poppins"
    } else {
        "Inter"
    };
    for (kind, size, bold) in [
        (egui::TextStyle::Body, a.font_size, false),
        (egui::TextStyle::Button, a.font_size, false),
        (egui::TextStyle::Small, (a.font_size - 2.0).max(9.0), false),
        (egui::TextStyle::Heading, a.font_size + 11.0, true),
    ] {
        style.text_styles.insert(
            kind,
            FontId::new(
                size,
                FontFamily::Name(
                    if bold {
                        format!("{family} Bold")
                    } else {
                        family.into()
                    }
                    .into(),
                ),
            ),
        );
    }
    if *original != style {
        ctx.set_style_of(mode, style);
    }
}

pub fn parse_custom(text: &str) -> Result<(String, BTreeMap<String, [u8; 3]>), String> {
    if text.len() > 65536 {
        return Err("Theme JSON exceeds 64 KiB".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("Invalid theme JSON: {e}"))?;
    let map = value.as_object().ok_or("Theme must be a JSON object")?;
    let base = map
        .get("extends")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            if map.get("mode").and_then(|v| v.as_str()) == Some("light") {
                "defaultLight"
            } else {
                "defaultDark"
            }
        });
    if !THEMES.iter().any(|(id, _)| *id == base) {
        return Err("Unknown base theme id".into());
    }
    let mut colors = BTreeMap::new();
    if let Some(value) = map.get("colors") {
        let entries = value.as_object().ok_or("colors must be an object")?;
        for (name, value) in entries {
            let text = value
                .as_str()
                .ok_or_else(|| format!("Color {name} must be a string"))?;
            colors.insert(
                name.clone(),
                parse_color(text).ok_or_else(|| {
                    format!("Invalid color for {name}: use #rgb, #rrggbb, or rgb(r,g,b)")
                })?,
            );
        }
    }
    Ok((base.into(), colors))
}
fn parse_color(text: &str) -> Option<[u8; 3]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let hex = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            hex.into()
        };
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let value = u32::from_str_radix(&hex, 16).ok()?;
        return Some([(value >> 16) as u8, (value >> 8) as u8, value as u8]);
    }
    let inner = text.strip_prefix("rgb(")?.strip_suffix(')')?;
    let parts = inner
        .split(',')
        .map(|s| s.trim().parse::<u8>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts.len() != 3 {
        return None;
    }
    Some([*parts.first()?, *parts.get(1)?, *parts.get(2)?])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_themes_reject_invalid_input_and_preserve_documented_colors() {
        assert!(parse_custom("[]").is_err());
        assert!(parse_custom(r##"{"colors":{"primary":"red"}}"##).is_err());
        let (base, colors) = parse_custom(
            r##"{"mode":"light","colors":{"primary":"#abc","background":"rgb(250,249,246)"}}"##,
        )
        .unwrap();
        assert_eq!(base, "defaultLight");
        assert_eq!(colors["primary"], [170, 187, 204]);
    }
}
