//! Local, line-synchronized lyrics. Reading is intended for a background worker.
use std::io::{self, Read};
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::tag::ItemKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: Option<i64>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synchronized: bool,
}

/// Prefers a nonempty same-name LRC, then falls back to embedded text lyrics.
/// An unreadable sidecar is reported instead of silently presenting missing lyrics.
pub fn read(path: &Path) -> Result<Lyrics, String> {
    let sidecar = path.with_extension("lrc");
    match std::fs::File::open(&sidecar) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(1_048_577)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 1_048_576 {
                return Err("歌词文件超过 1 MiB".into());
            }
            let text = decode(&bytes)?;
            let lyrics = parse(&text);
            if !lyrics.lines.is_empty() {
                return Ok(lyrics);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法读取歌词：{error}")),
    }
    let file = super::probe_path(
        path,
        ParseOptions::new()
            .read_properties(false)
            .read_cover_art(false),
    )
    .map_err(|error| error.to_string())?;
    for tag in file.tags() {
        for key in [ItemKey::Lyrics, ItemKey::UnsyncLyrics] {
            if let Some(text) = tag.get_string(key) {
                let lyrics = parse(text);
                if !lyrics.lines.is_empty() {
                    return Ok(lyrics);
                }
            }
        }
    }
    Ok(Lyrics::default())
}

fn decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return Err("歌词 UTF-16 编码不完整".into());
        }
        let little = bytes[0] == 0xff;
        let units: Vec<_> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "无法解码歌词，请使用 UTF-8 或 UTF-16".into())
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "无法解码歌词，请使用 UTF-8 或 UTF-16".into())
    }
}

#[must_use]
pub fn parse(text: &str) -> Lyrics {
    let mut timed = Vec::new();
    let mut plain = Vec::new();
    let mut offset: i64 = 0;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if let Some(value) = line
            .strip_prefix("[offset:")
            .and_then(|s| s.strip_suffix(']'))
        {
            if let Ok(value) = value.trim().parse() {
                offset = value;
            }
            continue;
        }
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(tag) = rest.strip_prefix('[').and_then(|s| s.split_once(']')) {
            let Some(time) = timestamp(tag.0) else { break };
            times.push(time);
            rest = tag.1;
        }
        if !times.is_empty() {
            for time in times {
                timed.push(LyricLine {
                    time_ms: Some(time),
                    text: rest.trim().to_owned(),
                });
            }
        } else if !line.is_empty() && !is_metadata(line) {
            plain.push(LyricLine {
                time_ms: None,
                text: line.to_owned(),
            });
        }
    }
    if timed.is_empty() {
        return Lyrics {
            lines: plain,
            synchronized: false,
        };
    }
    for line in &mut timed {
        // Positive LRC offset advances the displayed lyric relative to the audio.
        line.time_ms = line.time_ms.map(|time| time.saturating_sub(offset).max(0));
    }
    timed.sort_by_key(|line| line.time_ms);
    // Multiple lines at the same timestamp (e.g. translation) form one highlighted block.
    let mut lines: Vec<LyricLine> = Vec::new();
    for line in timed {
        if let Some(previous) = lines.last_mut().filter(|p| p.time_ms == line.time_ms) {
            if !line.text.is_empty() {
                if !previous.text.is_empty() {
                    previous.text.push('\n');
                }
                previous.text.push_str(&line.text);
            }
        } else {
            lines.push(line);
        }
    }
    Lyrics {
        lines,
        synchronized: true,
    }
}

fn is_metadata(line: &str) -> bool {
    ["ar", "al", "ti", "au", "by", "re", "ve", "length"]
        .iter()
        .any(|key| line.starts_with(&format!("[{key}:")) && line.ends_with(']'))
}

fn timestamp(value: &str) -> Option<i64> {
    let (minutes, seconds) = value.split_once(':')?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    if minutes.is_empty()
        || seconds.len() != 2
        || fraction.len() > 3
        || !minutes
            .bytes()
            .chain(seconds.bytes())
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let minutes: i64 = minutes.parse().ok()?;
    let seconds: i64 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32)
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1000)?
        .checked_add(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_offsets_translations_and_multiple_tags() {
        let lyrics = parse(
            "\u{feff}[ar:Artist]\n[00:02.34][00:01.5]hello\n[00:01.500]你好\n[00:03.000]\n[offset:200]",
        );
        assert!(lyrics.synchronized);
        assert_eq!(
            lyrics.lines[0],
            LyricLine {
                time_ms: Some(1300),
                text: "hello\n你好".into()
            }
        );
        assert_eq!(lyrics.lines[1].time_ms, Some(2140));
        assert_eq!(lyrics.lines[2].text, "");
    }

    #[test]
    fn plain_missing_and_invalid_timestamps() {
        assert_eq!(parse("[ti:歌名]\n第一行\n第二行").lines.len(), 2);
        assert!(parse("[ar:歌手]\n").lines.is_empty());
        for value in [
            "1:60",
            "-1:00",
            "999999999999999999999:00",
            "00:01.1234",
            "00:NaN",
        ] {
            assert_eq!(timestamp(value), None);
        }
        assert!(!parse("[副歌]\n正文").synchronized);
        assert_eq!(parse("[00:00.1]a\n[offset:500]").lines[0].time_ms, Some(0));
        assert_eq!(
            parse("[00:01]a\n[offset:-500]").lines[0].time_ms,
            Some(1500)
        );
    }

    #[test]
    fn local_file_precedence_embedded_fallback_and_missing_lyrics() {
        use lofty::tag::{Tag, TagExt, TagType};
        let directory = std::env::temp_dir().join(format!(
            "qingyin-lyrics-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("song.wav");
        crate::test_support::write_silence_wav(&path);
        assert!(read(&path).unwrap().lines.is_empty());
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::UnsyncLyrics, "内嵌歌词".into());
        tag.save_to_path(&path, lofty::config::WriteOptions::default())
            .unwrap();
        assert_eq!(read(&path).unwrap().lines[0].text, "内嵌歌词");
        std::fs::write(path.with_extension("lrc"), "[00:01.00]本地歌词").unwrap();
        assert_eq!(read(&path).unwrap().lines[0].text, "本地歌词");
        std::fs::write(path.with_extension("lrc"), "").unwrap();
        assert_eq!(read(&path).unwrap().lines[0].text, "内嵌歌词");
        std::fs::write(path.with_extension("lrc"), vec![b'a'; 1_048_577]).unwrap();
        assert!(read(&path).unwrap_err().contains("1 MiB"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unicode_decoding() {
        assert_eq!(
            decode(&[0xff, 0xfe, 0x60, 0x4f, 0x7d, 0x59]).unwrap(),
            "你好"
        );
        assert_eq!(decode(&[0xfe, 0xff, 0x4f, 0x60]).unwrap(), "你");
        assert!(decode(&[0xff, 0xfe, 0]).is_err());
        assert!(decode(&[0xff]).is_err());
    }
}
