/// "3:05" style for lists; switches to "1:02:03" past an hour.
pub fn clock(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".into();
    }
    let total = seconds as i64;
    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// Fixed-width "MM:SS" for the LCD; minutes keep counting past 59.
pub fn lcd(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "00:00".into();
    }
    let total = seconds as i64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// Frame to start playback from for a seek to `time`, clamped to the file.
pub fn seek_frame(time: f64, sample_rate: f64, length: i64) -> i64 {
    if sample_rate <= 0.0 || length <= 0 || !time.is_finite() {
        return 0;
    }
    ((time.max(0.0) * sample_rate).floor() as i64).min(length)
}

/// Elapsed time when playback was scheduled from `start_frame` and the player has rendered `rendered_frames` since.
pub fn elapsed(start_frame: i64, rendered_frames: i64, sample_rate: f64, length: i64) -> f64 {
    if sample_rate <= 0.0 {
        return 0.0;
    }
    (start_frame + rendered_frames.max(0)).min(length) as f64 / sample_rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_frame_clamps() {
        assert_eq!(seek_frame(1.5, 44_100.0, 441_000), 66_150);
        assert_eq!(seek_frame(-3.0, 44_100.0, 441_000), 0);
        assert_eq!(seek_frame(999.0, 44_100.0, 441_000), 441_000);
    }

    #[test]
    fn elapsed_time_after_seek() {
        assert!((elapsed(88_200, 44_100, 44_100.0, 441_000) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn formatting() {
        assert_eq!(clock(185.0), "3:05");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(lcd(3723.0), "62:03");
        assert_eq!(lcd(f64::NAN), "00:00");
    }
}
