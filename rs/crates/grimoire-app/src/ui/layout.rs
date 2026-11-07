//! Lays the main window out on the cell grid. Takes a plain description of the player (`View`) so it can be
//! tested, and rendered by `--render-ui`, without a running player.
//!
//! ```text
//! ┌─ GRIMOIRE ──────────────── ⛧ ─ – × ┐
//! │ > invoke  Moonrise.flac            │
//! │   ▓▓▓▓▓▓▓▓▓▓░░░░░░░░  02:41 / 05:12│
//! │   ‹ the coven ›  FLAC 44.1k        │
//! │   ▁▃▅▇▅▃▁▃▅▇▆▄▂▁                   │
//! │   ◁◁  ▶  ▷▷    vol ▮▮▮▮▯▯   ⇄ ⟲ ◈  │
//! ├────────────────────────────────────┤
//! │ 01  ✦ moonrise              5:12   │
//! └─ 12 spells · 48:10 ────────────────┘
//! ```

use grimoire_audio::PlayState;
use grimoire_core::{time_math, RepeatMode};

use super::screen::{CellRect, Rgba, Screen};
use super::theme::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Close,
    Minimize,
    /// Empty title bar: drag to move the window.
    Drag,
    Prompt,
    /// Seek bar; the fraction comes from where in the rect the pointer is.
    Seek,
    ToggleRemaining,
    Previous,
    PlayPause,
    Next,
    Volume,
    Shuffle,
    Repeat,
    Visualizer,
    Open,
    /// A playlist row, by index into `View::rows`.
    Row(usize),
    /// The × on a selected row: remove the selection.
    RemoveRow(usize),
    List,
    /// Opens or closes the settings panel (it takes the playlist's place).
    Settings,
    ToggleShortcuts,
    PreviousScene,
    NextScene,
    ToggleMeters,
    Reseed,
    ToggleDoomSound,
    OpenDoomControls,
    CloseDoomControls,
    /// A key slot on the Doom controls page: (row, slot 0 or 1).
    BindDoomKey(usize, usize),
    ResetDoomControls,
}

/// What the settings panel shows.
#[derive(Clone, Debug, Default)]
pub struct SettingsView {
    pub shortcuts: bool,
    pub scene: String,
    pub meters: bool,
    pub visualizer_open: bool,
    pub doom_sound: bool,
    /// Some while the Doom controls page is showing.
    pub doom_controls: Option<DoomControlsView>,
}

/// One line per Doom action: its label and the two keys bound to it (already formatted, "" for empty).
#[derive(Clone, Debug, Default)]
pub struct DoomControlsView {
    pub rows: Vec<(String, [String; 2])>,
    /// The slot waiting for a key press.
    pub capturing: Option<(usize, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct RowView {
    pub number: usize,
    pub title: String,
    pub duration: Option<f64>,
    pub current: bool,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct View {
    /// Seconds since launch, for blinking and shimmer.
    pub clock: f64,
    /// Bass envelope 0...1, for the pulsing sigil.
    pub beat: f32,
    pub state: PlayState,
    pub file_name: String,
    pub byline: String,
    pub format: String,
    pub elapsed: f64,
    pub duration: Option<f64>,
    pub show_remaining: bool,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub visualizer: bool,
    pub prompt: String,
    pub prompt_focused: bool,
    /// 0...1 bar heights (`BarDynamics` levels).
    pub spectrum: Vec<f32>,
    /// Visible playlist rows, already scrolled and filtered.
    pub rows: Vec<RowView>,
    pub track_count: usize,
    pub total_duration: f64,
    /// 0 just after a track change, rising to 1 when the glitch reveal has settled.
    pub reveal: f32,
    /// Some while the settings panel is open.
    pub settings: Option<SettingsView>,
}

/// How many playlist rows fit at this height (everything except the chrome around the list).
pub fn list_rows(rows: u16) -> usize {
    rows.saturating_sub(CHROME_ROWS) as usize
}

const CHROME_ROWS: u16 = 8;
pub const MIN_COLS: u16 = 40;
pub const MIN_ROWS: u16 = CHROME_ROWS + 2;

pub fn layout(view: &View, screen: &mut Screen<Action>) {
    let (w, h) = (screen.cols, screen.rows);
    if w < 12 || h < 4 {
        return;
    }
    let inner = w - 2;
    frame(view, screen);

    // Prompt.
    let row = 1;
    screen.hit(CellRect { x: 1, y: row, w: inner, h: 1 }, Action::Prompt);
    screen.text(2, row, ">", VIOLET, 1);
    if view.prompt_focused || !view.prompt.is_empty() {
        let n = screen.text_ellipsized(4, row, &view.prompt, GREEN, inner.saturating_sub(5));
        if view.prompt_focused && (view.clock * 1.6).fract() < 0.6 {
            screen.set(4 + n, row, '▌', GREEN);
        }
    } else {
        screen.text(4, row, "invoke", VIOLET_DIM, 6);
        if view.file_name.is_empty() {
            screen.text(12, row, "nothing summoned", GREEN_FAINT, inner.saturating_sub(12));
        } else {
            let name = glitched(&view.file_name, view.reveal, view.clock, 1);
            draw_glitched(screen, 12, row, &name, GREEN, inner.saturating_sub(12));
        }
    }

    // Seek bar and time.
    let row = 2;
    let time = match (view.show_remaining, view.duration) {
        (true, Some(d)) => format!("-{}", time_math::lcd(d - view.elapsed)),
        _ => time_math::lcd(view.elapsed),
    };
    let total = view.duration.map(time_math::lcd).unwrap_or_else(|| "--:--".into());
    let clock_text = format!("{time} / {total}");
    let blink_off = view.state == PlayState::Paused && (view.clock * 2.0).fract() < 0.5;
    let clock_start = screen.text_right(w - 2, row, &clock_text, if blink_off { GREEN_FAINT } else { GREEN });
    screen.hit(CellRect { x: clock_start, y: row, w: w - 2 - clock_start, h: 1 }, Action::ToggleRemaining);
    let bar_w = clock_start.saturating_sub(4 + 2);
    if bar_w >= 4 {
        let progress = view.duration.map_or(0.0, |d| if d > 0.0 { (view.elapsed / d).clamp(0.0, 1.0) } else { 0.0 });
        let filled = (progress * bar_w as f64).round() as u16;
        for i in 0..bar_w {
            let (ch, color) = if i < filled { ('━', GREEN) } else { ('─', GREEN_FAINT) };
            screen.set(4 + i, row, ch, color);
        }
        // The playhead: a small diamond that glows on the beat.
        let head = filled.min(bar_w - 1);
        screen.set(4 + head, row, '◆', with_alpha(GREEN, 0.75 + 0.25 * view.beat));
        screen.hit(CellRect { x: 4, y: row, w: bar_w, h: 1 }, Action::Seek);
    }

    // Byline and format.
    let row = 3;
    let mut col = 4;
    if !view.byline.is_empty() {
        let byline = glitched(&view.byline, view.reveal, view.clock, 2);
        screen.text(col, row, "‹ ", VIOLET_DIM, 2);
        let n = draw_glitched(screen, col + 2, row, &byline, GREEN_DIM, inner.saturating_sub(24));
        screen.text(col + 2 + n, row, " ›", VIOLET_DIM, 2);
        col += n + 6;
    }
    screen.text_ellipsized(col, row, &view.format, VIOLET_DIM, (w - 2).saturating_sub(col));

    // Spectrum in block glyphs.
    let row = 4;
    let bars = (inner.saturating_sub(5) as usize).min(view.spectrum.len().max(1) * 2);
    for i in 0..bars {
        let level = view.spectrum.get(i * view.spectrum.len() / bars.max(1)).copied().unwrap_or(0.0);
        let eighths = (level * 8.0).round() as u32;
        let ch = match eighths {
            0 => '·',
            8.. => '█',
            n => char::from_u32(0x2580 + n).unwrap_or('▁'),
        };
        let color = if eighths == 0 { GREEN_FAINT } else { mix(GREEN_DIM, VIOLET, i as f32 / bars as f32) };
        screen.set(4 + i as u16, row, ch, color);
    }

    // Transport, volume, toggles.
    let row = 5;
    let mut col = 4;
    let button = |screen: &mut Screen<Action>, col: &mut u16, label: &str, color: Rgba, action: Action| {
        let n = screen.text(*col, row, label, color, label.chars().count() as u16);
        screen.hit(CellRect { x: col.saturating_sub(1), y: row, w: n + 2, h: 1 }, action);
        *col += n + 2;
    };
    button(screen, &mut col, "◁◁", VIOLET, Action::Previous);
    let play = if view.state == PlayState::Playing { "❚❚" } else { "▶ " };
    button(screen, &mut col, play, GREEN, Action::PlayPause);
    button(screen, &mut col, "▷▷", VIOLET, Action::Next);
    col += 2;
    screen.text(col, row, "vol", VIOLET_DIM, 3);
    col += 4;
    let steps = 8u16;
    let lit = (view.volume * steps as f32).round() as u16;
    for i in 0..steps {
        screen.set(col + i, row, if i < lit { '▮' } else { '▯' }, if i < lit { GREEN } else { GREEN_FAINT });
    }
    screen.hit(CellRect { x: col, y: row, w: steps, h: 1 }, Action::Volume);

    let mut toggles: Vec<(String, bool, Action)> = vec![
        ("⇄".into(), view.shuffle, Action::Shuffle),
        (match view.repeat { RepeatMode::One => "⟲1", _ => "⟲" }.into(), view.repeat != RepeatMode::Off, Action::Repeat),
        ("◈".into(), view.visualizer, Action::Visualizer),
        ("⚙".into(), view.settings.is_some(), Action::Settings),
        ("+".into(), false, Action::Open),
    ];
    let mut end = w - 2;
    while let Some((label, on, action)) = toggles.pop() {
        let len = label.chars().count() as u16;
        let start = end.saturating_sub(len);
        if start <= col + steps + 1 {
            break;
        }
        screen.text(start, row, &label, if on { GREEN } else { VIOLET_DIM }, len);
        screen.hit(CellRect { x: start.saturating_sub(1), y: row, w: len + 2, h: 1 }, action);
        end = start.saturating_sub(2);
    }

    // Divider and the playlist.
    let row = 6;
    screen.set(0, row, '├', FRAME);
    screen.hline(1, row, inner, '─', FRAME);
    screen.set(w - 1, row, '┤', FRAME);

    let first = 7;
    let visible = list_rows(h);
    if let Some(settings) = &view.settings {
        if let Some(controls) = &settings.doom_controls {
            screen.text(2, 6, " doom controls ", VIOLET, 15);
            draw_doom_controls(controls, view.clock, screen, first, visible as u16);
        } else {
            screen.text(2, 6, " settings ", VIOLET, 10);
            draw_settings(settings, screen, first, visible as u16);
        }
        return;
    }
    screen.hit(CellRect { x: 1, y: first, w: inner, h: visible as u16 }, Action::List);
    for (i, track) in view.rows.iter().take(visible).enumerate() {
        let row = first + i as u16;
        if track.selected {
            screen.set_bg(CellRect { x: 1, y: row, w: inner, h: 1 }, SELECTION);
        }
        let color = if track.current { GREEN } else { GREEN_DIM };
        screen.text(2, row, &format!("{:02}", track.number), VIOLET_DIM, 4);
        if track.current {
            let pulse = 0.6 + 0.4 * view.beat;
            screen.set(6, row, '✦', with_alpha(VIOLET, pulse));
        }
        let duration = track.duration.map(time_math::clock).unwrap_or_default();
        // Room for the × that selected rows show after the duration.
        let duration_start = screen.text_right(w - 4, row, &duration, VIOLET_DIM);
        let title = if track.current { glitched(&track.title, view.reveal, view.clock, 3) } else { plain(&track.title) };
        draw_glitched(screen, 8, row, &title, color, duration_start.saturating_sub(10));
        screen.hit(CellRect { x: 1, y: row, w: inner, h: 1 }, Action::Row(i));
        if track.selected {
            screen.set(w - 3, row, '×', VIOLET);
            screen.hit(CellRect { x: w - 4, y: row, w: 3, h: 1 }, Action::RemoveRow(i));
        }
    }
    if view.rows.is_empty() && visible > 0 {
        let hint = if view.prompt.is_empty() { "drop audio here, or + to summon files" } else { "no spell answers" };
        screen.text(4, first, hint, GREEN_FAINT, inner.saturating_sub(4));
    }
}

fn draw_settings(settings: &SettingsView, screen: &mut Screen<Action>, first: u16, rows: u16) {
    let inner = screen.cols - 2;
    let value_col = 26.min(inner.saturating_sub(14));
    let mut row = first;
    let mut line = |screen: &mut Screen<Action>, label: &str, value: &str, on: bool, action: Action| {
        if row >= first + rows {
            return;
        }
        screen.text(4, row, label, GREEN_DIM, value_col.saturating_sub(5));
        let text = format!("‹ {value} ›");
        let n = screen.text(value_col, row, &text, if on { GREEN } else { VIOLET_DIM }, inner.saturating_sub(value_col));
        screen.hit(CellRect { x: 1, y: row, w: inner, h: 1 }, action);
        // On the scene row, the arrows step backward and forward.
        if action == Action::NextScene {
            screen.hit(CellRect { x: value_col.saturating_sub(1), y: row, w: 3, h: 1 }, Action::PreviousScene);
            screen.hit(CellRect { x: value_col + n.saturating_sub(2), y: row, w: 3, h: 1 }, Action::NextScene);
        }
        row += 1;
    };
    line(screen, "keyboard shortcuts", if settings.shortcuts { "on" } else { "off" }, settings.shortcuts, Action::ToggleShortcuts);
    line(screen, "visualizer scene", &settings.scene.to_lowercase(), true, Action::NextScene);
    line(screen, "band meters", if settings.meters { "on" } else { "off" }, settings.meters, Action::ToggleMeters);
    let reseed = if settings.visualizer_open { "cast" } else { "open ◈ first" };
    line(screen, "reseed the scene", reseed, settings.visualizer_open, Action::Reseed);
    line(screen, "doom sound", if settings.doom_sound { "on" } else { "off" }, settings.doom_sound, Action::ToggleDoomSound);
    line(screen, "doom controls", "edit", true, Action::OpenDoomControls);

    let help: &[&str] = if settings.shortcuts {
        &[
            "space play/pause · ←/→ seek (shift 30 s) · ↑/↓ select",
            "enter play · delete or × removes the selection · type to search",
            "⌘/ctrl ←/→ prev/next · o open · v visualizer",
            "visualizer: 1–7 or tab scenes · r reseed · m meters · f full",
            "doom: right-click the visualizer to leave it",
        ]
    } else {
        &["shortcuts are off: click the prompt to search", "⌘/ctrl+q quits · esc leaves full screen · × removes the selection"]
    };
    for (i, text) in help.iter().enumerate() {
        let y = row + 1 + i as u16;
        if y < first + rows {
            screen.text_ellipsized(4, y, text, GREEN_FAINT, inner.saturating_sub(4));
        }
    }
}

fn draw_doom_controls(view: &DoomControlsView, clock: f64, screen: &mut Screen<Action>, first: u16, rows: u16) {
    let inner = screen.cols - 2;
    let slot_width = 12u16;
    let first_slot = 22.min(inner.saturating_sub(2 * slot_width));
    let end = first + rows;
    let mut row = first;
    for (i, (label, keys)) in view.rows.iter().enumerate() {
        if row >= end {
            break;
        }
        screen.text(4, row, label, GREEN_DIM, first_slot.saturating_sub(5));
        for (slot, key) in keys.iter().enumerate() {
            let x = first_slot + slot as u16 * slot_width;
            let capturing = view.capturing == Some((i, slot));
            let (text, color) = if capturing {
                // Blinks while waiting for a key.
                (if (clock * 2.0).fract() < 0.6 { "press a key" } else { "" }.to_string(), GREEN)
            } else if key.is_empty() {
                ("·".to_string(), GREEN_FAINT)
            } else {
                (key.clone(), GREEN)
            };
            screen.text(x, row, "‹", VIOLET_DIM, 1);
            screen.text_ellipsized(x + 2, row, &text, color, slot_width - 5);
            screen.text(x + slot_width - 3, row, "›", VIOLET_DIM, 1);
            screen.hit(CellRect { x, y: row, w: slot_width - 1, h: 1 }, Action::BindDoomKey(i, slot));
        }
        row += 1;
    }
    if row + 1 < end {
        row += 1;
        let n = screen.text(4, row, "‹ reset to defaults ›", VIOLET_DIM, inner.saturating_sub(4));
        screen.hit(CellRect { x: 3, y: row, w: n + 2, h: 1 }, Action::ResetDoomControls);
        let back_x = 4 + n + 3;
        let m = screen.text(back_x, row, "‹ back ›", VIOLET, inner.saturating_sub(back_x));
        screen.hit(CellRect { x: back_x - 1, y: row, w: m + 2, h: 1 }, Action::CloseDoomControls);
        row += 1;
    }
    if row < end {
        let help = if view.capturing.is_some() {
            "press the key to use · delete clears the slot · esc cancels"
        } else {
            "click a slot, then press a key · menus always use arrows, enter and esc"
        };
        screen.text_ellipsized(4, row, help, GREEN_FAINT, inner.saturating_sub(4));
    }
}

fn frame(view: &View, screen: &mut Screen<Action>) {
    let (w, h) = (screen.cols, screen.rows);
    let bottom = h - 1;
    screen.set(0, 0, '┌', FRAME);
    screen.hline(1, 0, w - 2, '─', FRAME);
    screen.set(w - 1, 0, '┐', FRAME);
    for row in 1..bottom {
        screen.set(0, row, '│', FRAME);
        screen.set(w - 1, row, '│', FRAME);
    }
    screen.set(0, bottom, '└', FRAME);
    screen.hline(1, bottom, w - 2, '─', FRAME);
    screen.set(w - 1, bottom, '┘', FRAME);

    screen.hit(CellRect { x: 0, y: 0, w, h: 1 }, Action::Drag);
    screen.text(2, 0, " GRIMOIRE ", GREEN, 10);
    // Window buttons, then the sigil, which breathes with the bass.
    screen.text(w - 4, 0, " ×", VIOLET_DIM, 2);
    screen.hit(CellRect { x: w - 4, y: 0, w: 3, h: 1 }, Action::Close);
    screen.text(w - 7, 0, " – ", VIOLET_DIM, 3);
    screen.hit(CellRect { x: w - 7, y: 0, w: 3, h: 1 }, Action::Minimize);
    screen.text(w - 11, 0, " ⛧ ", with_alpha(VIOLET, 0.55 + 0.45 * view.beat), 3);

    let spells = if view.track_count == 1 { "1 spell".to_string() } else { format!("{} spells", view.track_count) };
    let footer = if view.track_count > 0 {
        format!(" {spells} · {} ", time_math::clock(view.total_duration))
    } else {
        " the grimoire is empty ".to_string()
    };
    screen.text(2, bottom, &footer, VIOLET_DIM, w.saturating_sub(4));
}

/// A string mid-reveal: each character is either settled or still a flickering rune.
struct Glitched(Vec<(char, bool)>);

fn plain(text: &str) -> Glitched {
    Glitched(text.chars().map(|c| (c, false)).collect())
}

/// Characters settle left to right with some scatter while `reveal` goes 0 → 1.
fn glitched(text: &str, reveal: f32, clock: f64, salt: u32) -> Glitched {
    if reveal >= 1.0 {
        return plain(text);
    }
    let count = text.chars().count().max(1) as f32;
    let flicker = (clock * 24.0) as u32;
    Glitched(
        text.chars()
            .enumerate()
            .map(|(i, c)| {
                let threshold = (i as f32 / count) * 0.7 + 0.3 * hash(i as u32, salt);
                if c == ' ' || reveal >= threshold {
                    (c, false)
                } else {
                    (RUNES[(hash(i as u32 + flicker * 31, salt) * RUNES.len() as f32) as usize % RUNES.len()], true)
                }
            })
            .collect(),
    )
}

fn draw_glitched(screen: &mut Screen<Action>, col: u16, row: u16, text: &Glitched, color: Rgba, max: u16) -> u16 {
    let len = text.0.len() as u16;
    let fits = len <= max;
    let count = if fits { len } else { max.saturating_sub(1) };
    for (i, &(ch, rune)) in text.0.iter().take(count as usize).enumerate() {
        screen.set(col + i as u16, row, ch, if rune { VIOLET } else { color });
    }
    if !fits && max > 0 {
        screen.set(col + count, row, '…', color);
        return max;
    }
    count
}

fn hash(n: u32, salt: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_536.0
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// A believable view for screenshots and tests.
pub fn sample_view() -> View {
    let titles = [
        ("Moonrise", 312.0),
        ("Ashen Bloom", 228.0),
        ("Hex of Tides", 260.0),
        ("Candle Static", 197.0),
        ("Nine Mirrors", 343.0),
        ("Salt Circle", 251.0),
        ("Lantern Protocol", 289.0),
        ("Vesper Loop", 214.0),
        ("The Glass Familiar", 406.0),
        ("Witchlight Carrier", 233.0),
        ("Sigil Drift", 275.0),
        ("Midnight Compiler", 318.0),
    ];
    View {
        clock: 12.3,
        beat: 0.7,
        state: PlayState::Playing,
        file_name: "Moonrise.flac".into(),
        byline: "the coven".into(),
        format: "FLAC 44.1k 16-bit 912kbps".into(),
        elapsed: 161.0,
        duration: Some(312.0),
        show_remaining: false,
        volume: 0.62,
        shuffle: false,
        repeat: RepeatMode::All,
        visualizer: true,
        prompt: String::new(),
        prompt_focused: false,
        spectrum: vec![
            0.55, 0.8, 0.95, 0.7, 0.62, 0.5, 0.58, 0.44, 0.4, 0.47, 0.36, 0.3, 0.33, 0.25, 0.2, 0.22, 0.15, 0.12, 0.08,
            0.05,
        ],
        rows: titles
            .iter()
            .enumerate()
            .map(|(i, (title, duration))| RowView {
                number: i + 1,
                title: title.to_lowercase(),
                duration: Some(*duration),
                current: i == 0,
                selected: i == 2,
            })
            .collect(),
        track_count: titles.len(),
        total_duration: titles.iter().map(|t| t.1).sum(),
        reveal: 1.0,
        settings: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(view: &View, cols: u16, rows: u16) -> Screen<Action> {
        let mut screen = Screen::new(cols, rows);
        layout(view, &mut screen);
        screen
    }

    #[test]
    fn sample_lays_out_like_the_mockup() {
        let screen = render(&sample_view(), 48, 16);
        assert!(screen.row_text(0).starts_with("┌─ GRIMOIRE ─"));
        assert!(screen.row_text(1).contains("> invoke  Moonrise.flac"));
        assert!(screen.row_text(2).contains("02:41 / 05:12"));
        assert!(screen.row_text(3).contains("‹ the coven ›"));
        assert!(screen.row_text(7).contains("01  ✦ moonrise"));
        assert!(screen.row_text(7).ends_with("5:12   │"));
        assert!(screen.row_text(15).starts_with("└─ 12 spells · 55:26 "));
    }

    #[test]
    fn controls_are_clickable_where_they_are_drawn() {
        let screen = render(&sample_view(), 48, 16);
        let find = |action: Action| screen.hits.iter().find(|h| h.action == action).map(|h| h.rect).unwrap();
        let play = find(Action::PlayPause);
        assert_eq!(screen.row_text(5).chars().nth(play.x as usize + 1), Some('❚'));
        assert_eq!(screen.hit_test(play.x + 1, 5).map(|h| h.action), Some(Action::PlayPause));
        assert_eq!(screen.hit_test(10, 9).map(|h| h.action), Some(Action::Row(2)));
        // The selected row (03) has a × that removes it.
        assert_eq!(screen.row_text(9).chars().nth(45), Some('×'));
        assert_eq!(screen.hit_test(45, 9).map(|h| h.action), Some(Action::RemoveRow(2)));
        assert!(!screen.row_text(8).contains('×'));
        assert_eq!(screen.hit_test(20, 0).map(|h| h.action), Some(Action::Drag));
        assert_eq!(screen.hit_test(46, 0).map(|h| h.action), Some(Action::Close));
        let seek = find(Action::Seek);
        assert!(seek.w > 10);
    }

    #[test]
    fn survives_tiny_and_huge_windows() {
        for (cols, rows) in [(1, 1), (12, 4), (MIN_COLS, MIN_ROWS), (300, 120)] {
            render(&sample_view(), cols, rows);
        }
        let mut empty = sample_view();
        empty.rows.clear();
        empty.track_count = 0;
        assert!(render(&empty, 48, 12).row_text(7).contains("drop audio here"));
    }

    #[test]
    fn settings_panel_replaces_the_playlist() {
        let mut view = sample_view();
        view.settings = Some(SettingsView {
            shortcuts: false,
            scene: "Three-Body".into(),
            meters: true,
            visualizer_open: true,
            doom_sound: false,
            doom_controls: None,
        });
        let screen = render(&view, 48, 16);
        assert!(screen.row_text(6).contains(" settings "));
        assert!(screen.row_text(7).contains("keyboard shortcuts") && screen.row_text(7).contains("‹ off ›"));
        assert!(screen.row_text(8).contains("‹ three-body ›"));
        assert!(!screen.row_text(7).contains("moonrise"));
        assert_eq!(screen.hit_test(10, 7).map(|h| h.action), Some(Action::ToggleShortcuts));
        let row = screen.row_text(8);
        let open = row.chars().position(|c| c == '‹').unwrap() as u16;
        let close = row.chars().position(|c| c == '›').unwrap() as u16;
        assert_eq!(screen.hit_test(open, 8).map(|h| h.action), Some(Action::PreviousScene));
        assert_eq!(screen.hit_test(close, 8).map(|h| h.action), Some(Action::NextScene));
        let gear = screen.row_text(5).chars().position(|c| c == '⚙').unwrap() as u16;
        assert_eq!(screen.hit_test(gear, 5).map(|h| h.action), Some(Action::Settings));
    }

    #[test]
    fn glitch_settles_into_the_real_text() {
        let mut view = sample_view();
        view.reveal = 0.0;
        let scrambled = render(&view, 48, 16).row_text(1);
        assert!(!scrambled.contains("Moonrise"));
        assert!(scrambled.chars().any(|c| RUNES.contains(&c)));
        view.reveal = 1.0;
        assert!(render(&view, 48, 16).row_text(1).contains("Moonrise.flac"));
    }
}
