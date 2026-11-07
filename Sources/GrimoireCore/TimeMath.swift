import Foundation

public enum TimeFormat {
    /// "3:05" style for lists; switches to "1:02:03" past an hour.
    public static func clock(_ seconds: TimeInterval) -> String {
        guard seconds.isFinite, seconds > 0 else { return "0:00" }
        let total = Int(seconds)
        let hours = total / 3600, minutes = (total % 3600) / 60, secs = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, secs)
            : String(format: "%d:%02d", minutes, secs)
    }

    /// Fixed-width "MM:SS" for the LCD; minutes keep counting past 59.
    public static func lcd(_ seconds: TimeInterval) -> String {
        guard seconds.isFinite, seconds > 0 else { return "00:00" }
        let total = Int(seconds)
        return String(format: "%02d:%02d", total / 60, total % 60)
    }
}

public enum SeekMath {
    /// Frame to start playback from for a seek to `time`, clamped to the file.
    public static func frame(for time: TimeInterval, sampleRate: Double, length: Int64) -> Int64 {
        guard sampleRate > 0, length > 0, time.isFinite else { return 0 }
        let frame = Int64((max(0, time) * sampleRate).rounded(.down))
        return min(frame, length)
    }

    /// Elapsed time when playback was scheduled from `startFrame` and the player has rendered `renderedFrames` since.
    public static func time(startFrame: Int64, renderedFrames: Int64, sampleRate: Double, length: Int64) -> TimeInterval {
        guard sampleRate > 0 else { return 0 }
        return Double(min(startFrame + max(0, renderedFrames), length)) / sampleRate
    }
}
