//! User-editable shortcuts use exact modifiers, so Alt/Shift do not trigger other actions.
use egui::{Key, Modifiers};

pub(super) fn parse(text: &str) -> Option<(Modifiers, Key)> {
    let mut parts = text.split('+').map(str::trim).peekable();
    let mut modifiers = Modifiers::NONE;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return Key::from_name(part).map(|key| (modifiers, key));
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "cmd" | "command" | "super" => modifiers.command = true,
            _ => return None,
        }
    }
    None
}

pub(super) fn consume(input: &mut egui::InputState, binding: &str) -> bool {
    let Some((modifiers, key)) = parse(binding) else {
        return false;
    };
    let mut matched = false;
    input.events.retain(|event| {
        let hit = matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers: actual, .. }
            if *pressed == key && actual.matches_exact(modifiers));
        matched |= hit;
        !hit
    });
    matched
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_empty_bindings_are_disabled() {
        assert!(parse("").is_none());
        assert!(parse("Ctrl+not-a-key").is_none());
        assert!(parse("Typo+Space").is_none());
        assert_eq!(
            parse("Ctrl+Shift+Space"),
            Some((Modifiers::CTRL | Modifiers::SHIFT, Key::Space))
        );
        assert_eq!(parse("F11"), Some((Modifiers::NONE, Key::F11)));
    }
}
