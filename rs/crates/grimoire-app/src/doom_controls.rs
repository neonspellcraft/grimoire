//! Which keyboard keys play Doom, editable in Grimoire's settings (⚙ → doom controls). Each action has two key
//! slots. Keys are stored by name: winit's named keys as their names ("ArrowUp", "Control"), anything else as the
//! lowercase character it types ("w", ",").

use std::collections::BTreeMap;

use grimoire_doom::keys;
use serde::{Deserialize, Serialize};
use winit::keyboard::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DoomAction {
    Forward,
    Back,
    TurnLeft,
    TurnRight,
    StrafeLeft,
    StrafeRight,
    Fire,
    Use,
    Run,
    /// Held to turn the turn keys into strafing (Doom's "strafe on").
    Strafe,
    Menu,
    Map,
}

impl DoomAction {
    pub const ALL: [DoomAction; 12] = [
        DoomAction::Forward,
        DoomAction::Back,
        DoomAction::TurnLeft,
        DoomAction::TurnRight,
        DoomAction::StrafeLeft,
        DoomAction::StrafeRight,
        DoomAction::Fire,
        DoomAction::Use,
        DoomAction::Run,
        DoomAction::Strafe,
        DoomAction::Menu,
        DoomAction::Map,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DoomAction::Forward => "forward",
            DoomAction::Back => "back",
            DoomAction::TurnLeft => "turn left",
            DoomAction::TurnRight => "turn right",
            DoomAction::StrafeLeft => "strafe left",
            DoomAction::StrafeRight => "strafe right",
            DoomAction::Fire => "fire",
            DoomAction::Use => "use / open",
            DoomAction::Run => "run",
            DoomAction::Strafe => "strafe (hold)",
            DoomAction::Menu => "menu",
            DoomAction::Map => "map",
        }
    }

    /// The key code the Doom engine's own default bindings expect for this action.
    pub fn doom_key(self) -> u8 {
        match self {
            DoomAction::Forward => keys::UP,
            DoomAction::Back => keys::DOWN,
            DoomAction::TurnLeft => keys::LEFT,
            DoomAction::TurnRight => keys::RIGHT,
            DoomAction::StrafeLeft => keys::STRAFE_LEFT,
            DoomAction::StrafeRight => keys::STRAFE_RIGHT,
            DoomAction::Fire => keys::FIRE,
            DoomAction::Use => keys::USE,
            DoomAction::Run => keys::RUN,
            DoomAction::Strafe => keys::STRAFE,
            DoomAction::Menu => keys::ESCAPE,
            DoomAction::Map => keys::TAB,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DoomControls {
    pub keys: BTreeMap<DoomAction, [Option<String>; 2]>,
}

impl Default for DoomControls {
    fn default() -> Self {
        let pair = |a: &str, b: Option<&str>| [Some(a.to_string()), b.map(str::to_string)];
        let keys = BTreeMap::from([
            (DoomAction::Forward, pair("w", Some("ArrowUp"))),
            (DoomAction::Back, pair("s", Some("ArrowDown"))),
            (DoomAction::TurnLeft, pair("ArrowLeft", None)),
            (DoomAction::TurnRight, pair("ArrowRight", None)),
            (DoomAction::StrafeLeft, pair("a", Some(","))),
            (DoomAction::StrafeRight, pair("d", Some("."))),
            (DoomAction::Fire, pair("Control", None)),
            (DoomAction::Use, pair("Space", Some("e"))),
            (DoomAction::Run, pair("Shift", None)),
            (DoomAction::Strafe, pair("Alt", None)),
            (DoomAction::Menu, pair("Escape", None)),
            (DoomAction::Map, pair("Tab", None)),
        ]);
        DoomControls { keys }
    }
}

impl DoomControls {
    pub fn slots(&self, action: DoomAction) -> [Option<String>; 2] {
        self.keys.get(&action).cloned().unwrap_or_default()
    }

    /// The actions a key is bound to.
    pub fn actions_for(&self, key: &str) -> Vec<DoomAction> {
        self.keys
            .iter()
            .filter(|(_, slots)| slots.iter().flatten().any(|k| k == key))
            .map(|(action, _)| *action)
            .collect()
    }

    /// Binds `key` to one slot of `action`, taking it off any other action first so a key does one thing.
    pub fn bind(&mut self, action: DoomAction, slot: usize, key: Option<String>) {
        if let Some(key) = &key {
            for slots in self.keys.values_mut() {
                for existing in slots.iter_mut() {
                    if existing.as_deref() == Some(key.as_str()) {
                        *existing = None;
                    }
                }
            }
        }
        self.keys.entry(action).or_default()[slot.min(1)] = key;
    }
}

/// The stored name of a key (see the module docs), or None for keys that can't be bound.
pub fn key_name(key: &Key) -> Option<String> {
    match key {
        Key::Named(named) => Some(format!("{named:?}")),
        Key::Character(text) => text.chars().next().map(|c| c.to_lowercase().to_string()),
        _ => None,
    }
}

/// How a key name reads on screen.
pub fn display(name: &str) -> String {
    match name {
        "ArrowUp" => "↑".into(),
        "ArrowDown" => "↓".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        "Control" => "ctrl".into(),
        "Shift" => "shift".into(),
        "Alt" => if cfg!(target_os = "macos") { "option".into() } else { "alt".into() },
        "Super" => if cfg!(target_os = "macos") { "cmd".into() } else { "win".into() },
        "Space" => "space".into(),
        "Escape" => "esc".into(),
        "Enter" => "enter".into(),
        "Tab" => "tab".into(),
        "Backspace" => "backspace".into(),
        "CapsLock" => "caps".into(),
        other => other.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::NamedKey;

    #[test]
    fn defaults_cover_every_action_and_round_trip() {
        let controls = DoomControls::default();
        for action in DoomAction::ALL {
            assert!(controls.slots(action)[0].is_some(), "{action:?} has no key");
        }
        let text = serde_json::to_string(&controls).unwrap();
        assert_eq!(serde_json::from_str::<DoomControls>(&text).unwrap(), controls);
    }

    #[test]
    fn binding_a_key_moves_it_off_other_actions() {
        let mut controls = DoomControls::default();
        assert_eq!(controls.actions_for("w"), vec![DoomAction::Forward]);
        controls.bind(DoomAction::Fire, 1, Some("w".into()));
        assert_eq!(controls.actions_for("w"), vec![DoomAction::Fire]);
        assert_eq!(controls.slots(DoomAction::Forward)[0], None);
        controls.bind(DoomAction::Fire, 1, None);
        assert!(controls.actions_for("w").is_empty());
    }

    #[test]
    fn key_names_and_display() {
        assert_eq!(key_name(&Key::Named(NamedKey::ArrowUp)).as_deref(), Some("ArrowUp"));
        assert_eq!(key_name(&Key::Character("W".into())).as_deref(), Some("w"));
        assert_eq!(display("ArrowUp"), "↑");
        assert_eq!(display("Control"), "ctrl");
        assert_eq!(display("w"), "w");
    }
}
