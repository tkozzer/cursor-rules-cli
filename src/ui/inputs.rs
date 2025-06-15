use super::AppAction;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;

/// Convert a raw `KeyEvent` from crossterm into a high-level [`AppAction`].
/// Returns `None` for keys that are not handled by the UI.
pub fn key_event_to_action(ev: &KeyEvent) -> Option<AppAction> {
    use KeyCode::*;
    match ev.code {
        Char('q') => Some(AppAction::Quit),
        Up | Char('k') => Some(AppAction::Up),
        Down | Char('j') => Some(AppAction::Down),
        Left | Char('h') => Some(AppAction::Left),
        Right | Char('l') => Some(AppAction::Right),
        Enter | Char('\r') => Some(AppAction::Select),
        Char(' ') => Some(AppAction::ToggleMark),
        Char('?') => Some(AppAction::Help),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn arrow_and_vim_keys_map_correctly() {
        let cases = vec![
            (KeyCode::Up, AppAction::Up),
            (KeyCode::Char('k'), AppAction::Up),
            (KeyCode::Down, AppAction::Down),
            (KeyCode::Char('j'), AppAction::Down),
            (KeyCode::Left, AppAction::Left),
            (KeyCode::Char('h'), AppAction::Left),
            (KeyCode::Right, AppAction::Right),
            (KeyCode::Char('l'), AppAction::Right),
            (KeyCode::Char(' '), AppAction::ToggleMark),
        ];

        for (code, expected) in cases {
            let ev = KeyEvent::new(code, KeyModifiers::NONE);
            assert_eq!(key_event_to_action(&ev), Some(expected));
        }
    }
}
