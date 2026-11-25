use pelhaim_board::{Mods, UiInput, UiKey};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Which modifier is the shortcut key. A parameter, not a `cfg`, so both mappings are
/// tested on any host (F50).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Other
        }
    }
}

pub fn map_key(key: &Key, mods: ModifiersState, platform: Platform) -> Option<(UiKey, Mods)> {
    let k = match key {
        Key::Named(n) => match n {
            NamedKey::Enter => UiKey::Enter,
            NamedKey::Escape => UiKey::Esc,
            NamedKey::Tab => UiKey::Tab,
            NamedKey::Backspace => UiKey::Backspace,
            NamedKey::ArrowLeft => UiKey::Left,
            NamedKey::ArrowRight => UiKey::Right,
            NamedKey::ArrowUp => UiKey::Up,
            NamedKey::ArrowDown => UiKey::Down,
            NamedKey::Home => UiKey::Home,
            NamedKey::End => UiKey::End,
            NamedKey::Space => UiKey::Space,
            _ => return None,
        },
        Key::Character(s) => {
            let mut cs = s.chars();
            match (cs.next(), cs.next()) {
                (Some(' '), None) => UiKey::Space,
                (Some(c), None) => UiKey::Char(c),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some((
        k,
        Mods {
            shift: mods.shift_key(),
            cmd: is_cmd(mods, platform),
        },
    ))
}

fn is_cmd(mods: ModifiersState, platform: Platform) -> bool {
    match platform {
        Platform::Mac => mods.super_key(),
        Platform::Other => mods.control_key(),
    }
}

/// The inputs for one key press, in order (F41). Text goes only to a console that was
/// already focused before the press: an unfocused `/` focuses the console and inserts the
/// `/` itself, so delivering its text too would type `//`.
pub fn press_inputs(
    key: &Key,
    text: Option<&str>,
    mods: ModifiersState,
    platform: Platform,
    console_focused: bool,
    composing: bool,
) -> Vec<UiInput> {
    let mut out: Vec<UiInput> = map_key(key, mods, platform)
        .map(|(k, m)| UiInput::Key(k, m))
        .into_iter()
        .collect();
    if let Some(t) = text
        && console_focused
        && !composing
        && !is_cmd(mods, platform)
        && !t.is_empty()
        && !t.chars().any(char::is_control)
    {
        out.push(UiInput::Text(t.into()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelhaim_board::{PaletteKind, SceneInput, ViewState, board_layout, build_scene, on_input};
    use pelhaim_core::Snapshot;
    use pelhaim_core::testkit::sample_board;

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }

    #[test]
    fn map_named_and_char_keys() {
        let none = ModifiersState::empty();
        for p in [Platform::Mac, Platform::Other] {
            assert_eq!(
                map_key(&Key::Named(NamedKey::Escape), none, p),
                Some((UiKey::Esc, Mods::default()))
            );
            assert_eq!(
                map_key(&ch("j"), none, p),
                Some((UiKey::Char('j'), Mods::default()))
            );
            assert_eq!(
                map_key(&Key::Named(NamedKey::Space), none, p),
                Some((UiKey::Space, Mods::default()))
            );
            assert_eq!(
                map_key(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT, p),
                Some((
                    UiKey::Tab,
                    Mods {
                        shift: true,
                        cmd: false
                    }
                ))
            );
            assert_eq!(map_key(&Key::Named(NamedKey::F5), none, p), None);
        }
        let cmd = Mods {
            shift: false,
            cmd: true,
        };
        assert_eq!(
            map_key(&ch("="), ModifiersState::SUPER, Platform::Mac),
            Some((UiKey::Char('='), cmd))
        );
        assert_eq!(
            map_key(&ch("="), ModifiersState::CONTROL, Platform::Other),
            Some((UiKey::Char('='), cmd))
        );
        assert_eq!(
            map_key(&ch("="), ModifiersState::CONTROL, Platform::Mac),
            Some((UiKey::Char('='), Mods::default()))
        );
        assert_eq!(
            map_key(&ch("="), ModifiersState::SUPER, Platform::Other),
            Some((UiKey::Char('='), Mods::default()))
        );
    }

    /// Feeds one press through `on_input` the way the app does, rebuilding the scene.
    fn press(view: &mut ViewState, snap: &Snapshot, key: Key, text: &str) {
        let inputs = press_inputs(
            &key,
            Some(text),
            ModifiersState::empty(),
            Platform::Mac,
            view.console_focused,
            false,
        );
        for ev in inputs {
            let scene = build_scene(&SceneInput {
                snap,
                view,
                viewport: (1600.0, 900.0),
                now_s: 0.0,
                palette: PaletteKind::Matte,
                reduced_motion: false,
            });
            let layout = board_layout(snap, 1600.0);
            on_input(view, &scene, snap, &layout, (1600.0, 900.0), 0.0, ev);
        }
    }

    #[test]
    fn one_key_press_inserts_one_char() {
        let snap = sample_board();
        let mut view = ViewState::default();
        press(&mut view, &snap, ch("/"), "/");
        assert!(view.console_focused);
        assert_eq!(view.console.text(), "/");
        for c in ["g", "o", "/", " "] {
            let key = if c == " " {
                Key::Named(NamedKey::Space)
            } else {
                ch(c)
            };
            press(&mut view, &snap, key, c);
        }
        assert_eq!(view.console.text(), "/go/ ");
        assert!(
            !view.paused,
            "Space in the console types, it does not freeze"
        );
    }

    #[test]
    fn text_is_held_back_while_composing_or_with_cmd() {
        let k = ch("a");
        let none = ModifiersState::empty();
        let text_of = |v: Vec<UiInput>| {
            v.into_iter()
                .filter(|e| matches!(e, UiInput::Text(_)))
                .count()
        };
        assert_eq!(
            text_of(press_inputs(
                &k,
                Some("a"),
                none,
                Platform::Mac,
                true,
                false
            )),
            1
        );
        assert_eq!(
            text_of(press_inputs(&k, Some("a"), none, Platform::Mac, true, true)),
            0
        );
        assert_eq!(
            text_of(press_inputs(
                &k,
                Some("a"),
                ModifiersState::SUPER,
                Platform::Mac,
                true,
                false
            )),
            0
        );
        assert_eq!(
            text_of(press_inputs(
                &Key::Named(NamedKey::Enter),
                Some("\r"),
                none,
                Platform::Mac,
                true,
                false
            )),
            0
        );
        assert_eq!(
            text_of(press_inputs(
                &k,
                Some("a"),
                none,
                Platform::Mac,
                false,
                false
            )),
            0
        );
    }
}
