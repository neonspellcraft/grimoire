//! Game controllers (USB or Bluetooth: Xbox, PlayStation, Switch Pro and most generic pads) via gilrs, turned
//! into a small, library-independent description for the scenes that use them (Doom).

use gilrs::{Axis, Button, EventType, Gilrs};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadInput {
    Button(PadButton, bool),
    /// Stick axes, -1...1 (up and right positive).
    Stick(PadStick, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadButton {
    /// A on Xbox, Cross on PlayStation, B on Nintendo (the bottom face button).
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    LeftStick,
    RightStick,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadStick {
    LeftX,
    LeftY,
    RightX,
    RightY,
}

pub struct Controllers {
    gilrs: Option<Gilrs>,
}

impl Controllers {
    pub fn new() -> Controllers {
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(e) => {
                eprintln!("grimoire: no game controller support: {e}");
                None
            }
        };
        Controllers { gilrs }
    }

    /// Everything that happened on any connected controller since the last call.
    pub fn poll(&mut self) -> Vec<PadInput> {
        let Some(gilrs) = &mut self.gilrs else { return Vec::new() };
        let mut inputs = Vec::new();
        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => inputs.extend(button_of(button).map(|b| PadInput::Button(b, true))),
                EventType::ButtonReleased(button, _) => inputs.extend(button_of(button).map(|b| PadInput::Button(b, false))),
                EventType::AxisChanged(axis, value, _) => {
                    let stick = match axis {
                        Axis::LeftStickX => Some(PadStick::LeftX),
                        Axis::LeftStickY => Some(PadStick::LeftY),
                        Axis::RightStickX => Some(PadStick::RightX),
                        Axis::RightStickY => Some(PadStick::RightY),
                        _ => None,
                    };
                    inputs.extend(stick.map(|s| PadInput::Stick(s, value)));
                }
                // Some pads report triggers only as analog values.
                EventType::ButtonChanged(button @ (Button::LeftTrigger2 | Button::RightTrigger2), value, _) => {
                    inputs.extend(button_of(button).map(|b| PadInput::Button(b, value > 0.5)));
                }
                _ => {}
            }
        }
        inputs
    }
}

fn button_of(button: Button) -> Option<PadButton> {
    Some(match button {
        Button::South => PadButton::South,
        Button::East => PadButton::East,
        Button::West => PadButton::West,
        Button::North => PadButton::North,
        Button::LeftTrigger => PadButton::LeftBumper,
        Button::RightTrigger => PadButton::RightBumper,
        Button::LeftTrigger2 => PadButton::LeftTrigger,
        Button::RightTrigger2 => PadButton::RightTrigger,
        Button::LeftThumb => PadButton::LeftStick,
        Button::RightThumb => PadButton::RightStick,
        Button::Start => PadButton::Start,
        Button::Select => PadButton::Select,
        Button::DPadUp => PadButton::Up,
        Button::DPadDown => PadButton::Down,
        Button::DPadLeft => PadButton::Left,
        Button::DPadRight => PadButton::Right,
        _ => return None,
    })
}
