//! External-keyboard shortcut routing.
//!
//! Touch devices with a hardware keyboard must behave like the desktop app, so this module does
//! not keep a binding list of its own: it turns key events into the engine's canonical chord text
//! and looks them up in the same [`ShortcutTable`] the desktop uses. Custom keymap presets and
//! rebinding therefore apply on mobile automatically.

use effectcraft_engine::shortcuts::{APP_SCOPE, Bindable, ShortcutTable, normalize};

/// Modifier keys held during a key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    /// Command (⌘) on Apple keyboards, the Windows / Meta key elsewhere.
    pub cmd: bool,
    pub alt: bool,
    pub shift: bool,
}

/// The canonical chord text for a key event (`Cmd+Shift+Z`), or `None` for an empty key.
/// Modifier order in the input never matters.
pub fn chord_text(mods: Modifiers, key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let mut s = String::new();
    for (on, name) in [(mods.ctrl, "Ctrl"), (mods.cmd, "Cmd"), (mods.alt, "Alt"), (mods.shift, "Shift")] {
        if on {
            s.push_str(name);
            s.push('+');
        }
    }
    s.push_str(key);
    normalize(&s)
}

/// Finds the command a chord runs.
///
/// `active_scopes` are the panels that currently have focus (`"Timeline"`, `"Composition"`, ...).
/// A binding scoped to an active panel wins over an application-wide one on the same chord.
/// Bindings of inactive panels never fire.
pub fn resolve<'a>(table: &'a ShortcutTable, chord: &str, active_scopes: &[&str]) -> Option<&'a Bindable> {
    let wanted = normalize(chord)?;
    let mut app_hit = None;
    for (keys, binding) in table.bindings() {
        if normalize(keys).as_deref() != Some(wanted.as_str()) {
            continue;
        }
        if binding.scope == APP_SCOPE {
            app_hit = app_hit.or(Some(binding));
        } else if active_scopes.contains(&binding.scope.as_str()) {
            return Some(binding);
        }
    }
    app_hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use effectcraft_engine::shortcuts::Keymaps;
    use serde_json::Value;

    fn bind(key: &str, scope: &str) -> Bindable {
        Bindable { key: key.into(), command: key.into(), params: Value::Null, label: key.into(), path: vec![], scope: scope.into(), defaults: vec![] }
    }

    fn table(entries: &[(&str, &str, &str)]) -> ShortcutTable {
        let mut t = ShortcutTable::default();
        for (id, scope, keys) in entries {
            t.bindables.push(bind(id, scope));
            t.keys.insert((*id).into(), vec![(*keys).into()]);
        }
        t
    }

    #[test]
    fn modifier_order_does_not_matter() {
        let a = chord_text(Modifiers { ctrl: true, shift: true, ..Modifiers::default() }, "z");
        assert_eq!(a, normalize("shift+ctrl+z"));
        assert!(a.is_some());
        assert_eq!(chord_text(Modifiers::default(), "  "), None);
    }

    #[test]
    fn every_default_application_shortcut_resolves_from_its_own_chord() {
        let t = ShortcutTable::build(&Keymaps::default(), &[]);
        let app: Vec<_> = t.bindings().into_iter().filter(|(_, b)| b.scope == APP_SCOPE).collect();
        assert!(!app.is_empty(), "the default keymap binds application shortcuts");
        for (keys, b) in app {
            let hit = resolve(&t, keys, &[]).unwrap_or_else(|| panic!("{keys} ({}) did not resolve", b.label));
            assert!(t.keys.get(&hit.key).is_some_and(|ks| ks.iter().any(|k| normalize(k) == normalize(keys))));
        }
    }

    #[test]
    fn a_focused_panel_beats_the_application_scope() {
        let t = table(&[("app.delete", APP_SCOPE, "Delete"), ("timeline.delete_keys", "Timeline", "Delete")]);
        assert_eq!(resolve(&t, "delete", &["Timeline"]).map(|b| b.key.as_str()), Some("timeline.delete_keys"));
        assert_eq!(resolve(&t, "delete", &[]).map(|b| b.key.as_str()), Some("app.delete"));
        assert_eq!(resolve(&t, "delete", &["Composition"]).map(|b| b.key.as_str()), Some("app.delete"));
    }

    #[test]
    fn inactive_panel_shortcuts_do_not_fire() {
        let t = table(&[("timeline.next_key", "Timeline", "K")]);
        assert!(resolve(&t, "k", &[]).is_none());
        assert!(resolve(&t, "k", &["Composition"]).is_none());
        assert!(resolve(&t, "k", &["Timeline"]).is_some());
    }

    #[test]
    fn unknown_or_empty_chords_resolve_to_nothing() {
        let t = ShortcutTable::build(&Keymaps::default(), &[]);
        assert!(resolve(&t, "", &["Timeline"]).is_none());
        assert!(resolve(&t, "Ctrl+Alt+Shift+Cmd+F19", &["Timeline", "Composition"]).is_none());
    }

    #[test]
    fn rebinding_through_a_preset_is_honoured() {
        let t = table(&[("layer.solo", APP_SCOPE, "Ctrl+Alt+S")]);
        assert!(resolve(&t, "alt+ctrl+s", &[]).is_some());
        assert!(resolve(&t, "S", &[]).is_none());
    }
}
