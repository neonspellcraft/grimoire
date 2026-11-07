//! Needs a real audio output, so it's ignored by default (CI machines have none). Plays muted.
//!
//!     cargo test -p grimoire-audio --test device -- --ignored --nocapture

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use grimoire_audio::{PlayState, Player, PlayerEvent};

/// Writes `seconds` of a quiet 32-bit float stereo tone at `rate`.
fn write_tone(path: &Path, rate: u32, seconds: f64) {
    let frames = (rate as f64 * seconds) as u32;
    let mut data = Vec::new();
    for n in 0..frames {
        let s = (0.05 * (2.0 * std::f64::consts::PI * 440.0 * n as f64 / rate as f64).sin()) as f32;
        data.extend(s.to_le_bytes());
        data.extend(s.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(3u16.to_le_bytes()); // IEEE float
    wav.extend(2u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 8).to_le_bytes());
    wav.extend(8u16.to_le_bytes());
    wav.extend(32u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(path, wav).unwrap();
}

fn wait_for_event(player: &mut Player, limit: Duration) -> Option<PlayerEvent> {
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Some(event) = player.tick() {
            return Some(event);
        }
        sleep(Duration::from_millis(10));
    }
    None
}

#[test]
#[ignore]
fn plays_seeks_pauses_and_finishes() {
    let dir = std::env::temp_dir().join(format!("grimoire-device-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 44.1 kHz on a (usually) 48 kHz device exercises the resampler too.
    let path = dir.join("tone.wav");
    write_tone(&path, 44_100, 3.0);

    let mut player = Player::new().expect("audio output");
    player.set_volume(0.0);
    println!("output rate {:?}", player.output_sample_rate());
    let info = player.load(&path, true).unwrap();
    assert!((info.duration.unwrap() - 3.0).abs() < 1e-6);

    sleep(Duration::from_millis(600));
    let t = player.current_time();
    println!("after 0.6 s: {t:.3}");
    assert!((0.4..0.75).contains(&t), "time should advance in real time, got {t}");
    assert!(player.tap().position().written > 10_000, "the tap should be filling");

    player.pause();
    let paused_at = player.current_time();
    sleep(Duration::from_millis(300));
    assert!((player.current_time() - paused_at).abs() < 0.03, "paused time must hold");
    player.play();

    player.seek(2.0);
    sleep(Duration::from_millis(300));
    let t = player.current_time();
    println!("0.3 s after seeking to 2.0: {t:.3}");
    assert!((2.15..2.45).contains(&t), "got {t}");

    let event = wait_for_event(&mut player, Duration::from_secs(3));
    assert_eq!(event, Some(PlayerEvent::Finished));
    assert_eq!(player.state(), PlayState::Stopped);
    std::fs::remove_dir_all(dir).ok();
}
