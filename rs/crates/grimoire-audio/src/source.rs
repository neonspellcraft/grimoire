//! Opening and decoding audio files with symphonia, plus tag and duration reading (replaces `TrackLoader`).

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::Error;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileInfo {
    /// Seconds, when the container says.
    pub duration: Option<f64>,
    pub sample_rate: u32,
    pub channels: usize,
    /// Average kbps for compressed formats.
    pub bitrate: Option<u32>,
    /// Short format name for the info display, e.g. "FLAC", "MP3".
    pub format: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// An open file positioned somewhere in its stream, handing out planar f32 audio.
pub struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    pub info: FileInfo,
    buffer: Option<AudioBuffer<f32>>,
    /// Frames still to drop after an accurate seek landed before the requested time.
    skip: u64,
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, Error> {
        let file = File::open(path).map_err(|e| Error::Open(format!("{}: {e}", path.display())))?;
        let file_bytes = file.metadata().map(|m| m.len()).ok();
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let mut probed = symphonia::default::get_probe()
            .format(&hint, stream, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
            .map_err(|e| Error::Open(format!("{}: {e}", path.display())))?;

        let (mut title, mut artist) = (None, None);
        // Tags before the container (ID3v2 on MP3), then the container's own (Vorbis comments, RIFF INFO), which win.
        if let Some(revision) = probed.metadata.get().as_ref().and_then(|m| m.current().cloned()) {
            read_tags(&revision, &mut title, &mut artist);
        }
        let mut format = probed.format;
        if let Some(revision) = format.metadata().current() {
            read_tags(revision, &mut title, &mut artist);
        }

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| Error::Open(format!("{}: no audio track", path.display())))?;
        let params = &track.codec_params;
        let sample_rate = params.sample_rate.ok_or_else(|| Error::Open("unknown sample rate".into()))?;
        let channels = params.channels.map(|c| c.count()).unwrap_or(2);
        let duration = params.n_frames.map(|n| n as f64 / sample_rate as f64);
        let codec = symphonia::default::get_codecs().get_codec(params.codec).map(|d| d.short_name).unwrap_or("?");
        let format_name = match codec {
            "flac" => "FLAC".to_string(),
            "mp3" => "MP3".to_string(),
            pcm if pcm.starts_with("pcm") => match hint_ext(path).as_str() {
                "aif" | "aiff" | "aifc" => "AIFF".to_string(),
                _ => "WAV".to_string(),
            },
            other => other.to_uppercase(),
        };
        let bitrate = match (file_bytes, duration) {
            (Some(bytes), Some(seconds)) if seconds > 0.0 => Some((bytes as f64 * 8.0 / seconds / 1000.0).round() as u32),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|e| Error::Open(format!("{}: {e}", path.display())))?;
        let track_id = track.id;

        Ok(Source {
            format,
            decoder,
            track_id,
            info: FileInfo { duration, sample_rate, channels, bitrate, format: format_name, title, artist },
            buffer: None,
            skip: 0,
        })
    }

    /// Jumps to `seconds` (sample-accurate).
    pub fn seek(&mut self, seconds: f64) -> Result<(), Error> {
        self.skip = 0;
        let time = Time::from(seconds.max(0.0));
        match self.format.seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) }) {
            Ok(seeked) => {
                self.skip = seeked.required_ts.saturating_sub(seeked.actual_ts);
                self.decoder.reset();
                Ok(())
            }
            Err(e) => Err(Error::Decode(e.to_string())),
        }
    }

    /// Decodes the next packet. Calls `output` with one slice per channel (all the same length).
    /// Returns false at the end of the stream.
    pub fn next_chunk(&mut self, mut output: impl FnMut(&[&[f32]])) -> Result<bool, Error> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(Error::Decode(e.to_string())),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // A corrupt frame shouldn't stop the track; skip it like other players do.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(Error::Decode(e.to_string())),
            };
            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            let buffer = convert(&mut self.buffer, decoded);
            let skip = (self.skip as usize).min(frames);
            self.skip -= skip as u64;
            if skip == frames {
                continue;
            }
            let channels: Vec<&[f32]> = (0..buffer.spec().channels.count()).map(|c| &buffer.chan(c)[skip..]).collect();
            output(&channels);
            return Ok(true);
        }
    }
}

/// Converts any decoded sample format to planar f32, reusing one buffer.
fn convert<'b>(slot: &'b mut Option<AudioBuffer<f32>>, decoded: AudioBufferRef<'_>) -> &'b AudioBuffer<f32> {
    let needed = decoded.capacity() as u64;
    let spec = *decoded.spec();
    let fits = slot.as_ref().is_some_and(|b| b.capacity() as u64 >= needed && b.spec() == &spec);
    if !fits {
        *slot = Some(AudioBuffer::new(needed, spec));
    }
    let buffer = slot.as_mut().unwrap();
    decoded.convert(buffer);
    buffer
}

fn hint_ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn read_tags(revision: &MetadataRevision, title: &mut Option<String>, artist: &mut Option<String>) {
    for tag in revision.tags() {
        let value = tag.value.to_string();
        if value.trim().is_empty() {
            continue;
        }
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => *title = Some(value),
            Some(StandardTagKey::Artist) => *artist = Some(value),
            _ => {}
        }
    }
}

/// Reads tags and duration without starting playback (for filling in playlist rows).
pub fn read_info(path: &Path) -> Result<FileInfo, Error> {
    let mut info = Source::open(path)?.info;
    if info.duration.is_none() {
        info.duration = scan_duration(path);
    }
    Ok(info)
}

/// Counts frames by decoding the whole file. Only for files whose container doesn't state a length
/// (e.g. MP3s without a Xing/Info header).
fn scan_duration(path: &Path) -> Option<f64> {
    let mut source = Source::open(path).ok()?;
    let rate = source.info.sample_rate as f64;
    let mut frames = 0usize;
    while source.next_chunk(|channels| frames += channels.first().map_or(0, |c| c.len())).ok()? {}
    Some(frames as f64 / rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit stereo WAV where sample n of the left channel is n (mod 32768), so positions can be checked exactly.
    fn write_ramp_wav(path: &Path, rate: u32, frames: u32) {
        let mut data = Vec::new();
        for n in 0..frames {
            let left = (n % 32_768) as i16;
            data.extend(left.to_le_bytes());
            data.extend((-left).to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes()); // PCM
        wav.extend(2u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 4).to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(data);
        std::fs::write(path, wav).unwrap();
    }

    fn first_left_sample(source: &mut Source) -> i32 {
        let mut value = None;
        source.next_chunk(|channels| value = Some(channels[0][0])).unwrap();
        (value.unwrap() * 32_768.0).round() as i32
    }

    #[test]
    fn decodes_wav_and_seeks_sample_accurately() {
        let dir = std::env::temp_dir().join(format!("grimoire-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ramp.wav");
        write_ramp_wav(&path, 44_100, 30_000);

        let mut source = Source::open(&path).unwrap();
        assert_eq!(source.info.sample_rate, 44_100);
        assert_eq!(source.info.channels, 2);
        assert_eq!(source.info.format, "WAV");
        assert!((source.info.duration.unwrap() - 30_000.0 / 44_100.0).abs() < 1e-9);

        let mut total = 0;
        let mut right_is_negated = true;
        while source
            .next_chunk(|ch| {
                total += ch[0].len();
                right_is_negated &= ch[0].iter().zip(ch[1]).all(|(l, r)| (l + r).abs() < 1e-6);
            })
            .unwrap()
        {}
        assert_eq!(total, 30_000);
        assert!(right_is_negated);

        source.seek(12_345.0 / 44_100.0).unwrap();
        assert_eq!(first_left_sample(&mut source), 12_345);
        source.seek(0.0).unwrap();
        assert_eq!(first_left_sample(&mut source), 0);
        std::fs::remove_dir_all(dir).ok();
    }
}
