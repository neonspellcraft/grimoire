//! Reads Vorbis comments (TITLE, ARTIST, …) from a FLAC file's metadata blocks.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Upper-cased tag names mapped to their first value, or None if the file isn't FLAC or has no comments.
pub fn read(path: &Path) -> Option<HashMap<String, String>> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    let mut header = [0u8; 4];
    while file.read_exact(&mut header).is_ok() {
        let is_last = header[0] & 0x80 != 0;
        let kind = header[0] & 0x7F;
        let length = (header[1] as usize) << 16 | (header[2] as usize) << 8 | header[3] as usize;
        if kind == 4 {
            let mut block = vec![0u8; length];
            file.read_exact(&mut block).ok()?;
            return parse_vorbis_comment(&block);
        }
        if is_last {
            return None;
        }
        file.seek(SeekFrom::Current(length as i64)).ok()?;
    }
    None
}

/// Vorbis comment block: little-endian vendor length + vendor, count, then `length + "KEY=value"` entries.
pub fn parse_vorbis_comment(data: &[u8]) -> Option<HashMap<String, String>> {
    let mut reader = Reader { data, position: 0 };
    let vendor = reader.u32()? as usize;
    reader.bytes(vendor)?;
    let count = reader.u32()?;
    let mut tags = HashMap::new();
    for _ in 0..count {
        let Some(length) = reader.u32() else { break };
        let Some(bytes) = reader.bytes(length as usize) else { break };
        let Ok(entry) = std::str::from_utf8(bytes) else { continue };
        let Some((key, value)) = entry.split_once('=') else { continue };
        tags.entry(key.to_uppercase()).or_insert_with(|| value.to_owned());
    }
    Some(tags)
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Option<u32> {
        self.bytes(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(count)?;
        let slice = self.data.get(self.position..end)?;
        self.position = end;
        Some(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(value: usize) -> Vec<u8> {
        (value as u32).to_le_bytes().to_vec()
    }

    #[test]
    fn parses_vorbis_comment() {
        let mut block = le(3);
        block.extend(b"abc");
        block.extend(le(2));
        for entry in ["title=Moonrise", "ARTIST=The Coven"] {
            block.extend(le(entry.len()));
            block.extend(entry.as_bytes());
        }
        let tags = parse_vorbis_comment(&block).unwrap();
        assert_eq!(tags["TITLE"], "Moonrise");
        assert_eq!(tags["ARTIST"], "The Coven");
    }

    #[test]
    fn reads_tags_from_file() {
        let Ok(path) = std::env::var("GRIMOIRE_TEST_FLAC") else {
            eprintln!("skipped: set GRIMOIRE_TEST_FLAC to a tagged .flac to run this");
            return;
        };
        assert!(read(Path::new(&path)).and_then(|t| t.get("TITLE").cloned()).is_some());
    }
}
