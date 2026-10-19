//! Sound effects.
//!
//! The original bundled four MP3 files and played them from the renderer.
//! Instead of shipping the same media files we synthesise the built-in tones
//! at runtime and play anything the user provides (file path or URL) through
//! the Windows MCI player.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows::Win32::Media::Multimedia::mciSendStringW;

/// 1 = 默认音效, 2 = 小黑子音效, 3 = 自定义 (mirrors `sounds.type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Default = 0,
    Ikun = 1,
    Custom = 2,
}

static MCI_ALIAS: Mutex<u32> = Mutex::new(0);

/// Render a small PCM WAV file in memory (16 bit mono).
fn synth_wav(freqs: &[(f32, f32)], sample_rate: u32) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    for (freq, secs) in freqs {
        let n = (sample_rate as f32 * secs) as usize;
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            // Simple decaying sine so the blip is pleasant.
            let env = (-6.0 * t).exp();
            let v = (t * freq * std::f32::consts::TAU).sin() * env * 0.35;
            samples.push((v * i16::MAX as f32) as i16);
        }
    }

    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Built-in "copy" blip: two rising notes.
fn copy_wav() -> Vec<u8> {
    synth_wav(&[(660.0, 0.05), (990.0, 0.06)], 44_100)
}

/// Built-in "paste" blip: two falling notes.
fn paste_wav() -> Vec<u8> {
    synth_wav(&[(880.0, 0.05), (587.0, 0.07)], 44_100)
}

/// The playful alternative ("小黑子音效"): a short three-note motif.
fn ikun_wav() -> Vec<u8> {
    synth_wav(&[(523.0, 0.07), (659.0, 0.07), (784.0, 0.12)], 44_100)
}

fn play_memory(wav: &[u8]) {
    // Owned buffer: `SND_MEMORY` requires the data to outlive playback.
    let leaked: &'static [u8] = Box::leak(wav.to_vec().into_boxed_slice());
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(leaked.as_ptr() as *const u16),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

/// Send a command to the MCI player.
fn mci(cmd: &str) -> u32 {
    let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { mciSendStringW(PCWSTR(wide.as_ptr()), None, None) }
}

/// Play a file through MCI (supports mp3 and wav).
pub fn play_file(path: &Path) -> bool {
    let alias = format!("clipassistant{}", next_alias());
    let open = format!("open \"{}\" type mpegvideo alias {}", path.display(), alias);
    if mci(&open) != 0 {
        // Fall back to a plain open which handles WAV through waveaudio.
        let open = format!("open \"{}\" alias {}", path.display(), alias);
        if mci(&open) != 0 {
            return false;
        }
    }
    let ok = mci(&format!("play {alias}")) == 0;
    let alias_owned = alias.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        mci(&format!("close {alias_owned}"));
    });
    ok
}

fn next_alias() -> u32 {
    let mut id = MCI_ALIAS.lock().unwrap();
    *id += 1;
    *id
}

/// Play the sound configured for `kind`.
pub fn play(kind: SoundKind, custom_url: &str) {
    match kind {
        SoundKind::Default => play_memory(&copy_wav()),
        SoundKind::Ikun => play_memory(&ikun_wav()),
        SoundKind::Custom => {
            if custom_url.is_empty() {
                return;
            }
            if custom_url.starts_with("http") {
                if let Some(path) = crate::api::download_to_cache(custom_url) {
                    play_file(&path);
                }
            } else {
                play_file(Path::new(custom_url));
            }
        }
    }
}

/// Distinguish copy and paste so the built-in tones differ.
pub fn play_copy(kind: SoundKind, custom_url: &str) {
    match kind {
        SoundKind::Default => play_memory(&copy_wav()),
        _ => play(kind, custom_url),
    }
}

pub fn play_paste(kind: SoundKind, custom_url: &str) {
    match kind {
        SoundKind::Default => play_memory(&paste_wav()),
        _ => play(kind, custom_url),
    }
}

/// Cache directory for downloaded custom sounds.
pub fn cache_dir() -> PathBuf {
    let dir = crate::data_dir().join("sounds");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
