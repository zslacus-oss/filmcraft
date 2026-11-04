//! External speech engine: speech-to-text by a separate program, through files.
//!
//! FilmCraft's own recognisers run in-process ([`filmcraft_speech`]). This module lets a host plug
//! in any recogniser that ships as its own executable (a GPU whisper.cpp build, a recogniser tuned
//! for one language, a commercial engine) without linking it: FilmCraft writes the audio to a WAV
//! file, runs the program, and reads the words back from a JSON file.
//!
//! ## Protocol (format version 1)
//!
//! FilmCraft runs `<engine> --subtitle-file <audio.wav> --out <result.json>` and waits. The WAV is
//! mono, 16 kHz, 16-bit PCM. Exit code 0 means `result.json` holds:
//!
//! ```json
//! {
//!   "format_version": 1,
//!   "language": "hu",
//!   "word_timing": "word",
//!   "words": [{"text": "Jó", "start_ms": 120, "end_ms": 380}, …],
//!   "warnings": ["…"]
//! }
//! ```
//!
//! Times are milliseconds from the first sample. `word_timing` is `"word"` when the engine timed
//! every word, anything else (`"segment-estimated"`) when word times were estimated inside longer
//! segments; estimated words get a lower confidence so the Text panel can show them as such. Any
//! other exit code is a failure; the engine's stderr becomes the error message.
//!
//! ## Finding the engine
//!
//! [`detect`] looks, in order, at the `FILMCRAFT_SPEECH_ENGINE` environment variable (a path to
//! the executable), next to the running FilmCraft executable, then at the per-user install of the
//! LIKE Diktálás engine on Windows. Nothing found means FilmCraft keeps its own recognisers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Deserialize;

use filmcraft_project::{Transcript, Word};
use filmcraft_speech::{Options, ProgressFn, SpeechError, Transcriber};
use filmcraft_time::{TICKS_PER_SECOND, Tick};

/// Environment variable naming the engine executable.
pub const ENGINE_ENV: &str = "FILMCRAFT_SPEECH_ENGINE";

/// File name of the LIKE Diktálás engine.
const LIKE_ENGINE_EXE: &str = if cfg!(windows) { "like-diktalas.exe" } else { "like-diktalas" };

/// The protocol version this module reads.
pub const FORMAT_VERSION: u32 = 1;

/// Confidence given to words whose time the engine only estimated.
const ESTIMATED_CONFIDENCE: f32 = 0.5;

/// Longest audio sent to an engine (10 hours of 16 kHz mono); longer media is refused rather than
/// written as a multi-gigabyte temporary file.
const MAX_SAMPLES: usize = 16_000 * 60 * 60 * 10;

/// Windows: run the engine without flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The engine executable, if one is installed (see the module docs for the search order).
pub fn detect() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENGINE_ENV).map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(p) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join(LIKE_ENGINE_EXE))).filter(|p| p.is_file()) {
        return Some(p);
    }
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        let p = PathBuf::from(local).join("LIKE Diktálás").join(LIKE_ENGINE_EXE);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// A [`Transcriber`] that runs an external engine program.
pub struct ExternalTranscriber {
    pub exe: PathBuf,
}

impl ExternalTranscriber {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    /// The installed engine, if any.
    pub fn detect() -> Option<Self> {
        detect().map(Self::new)
    }
}

impl Transcriber for ExternalTranscriber {
    fn id(&self) -> String {
        let stem = self.exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "external".into());
        format!("external:{stem}")
    }

    fn transcribe(&self, audio: &[f32], _opts: &Options, progress: ProgressFn) -> Result<Transcript, SpeechError> {
        if !progress(0.05, "Transcribing with the external speech engine") {
            return Err(SpeechError::Cancelled);
        }
        let mut t = transcribe_audio(&self.exe, audio, None).map_err(SpeechError::Io)?;
        t.source = self.id();
        Ok(t)
    }
}

/// Transcribe mono 16 kHz `audio` with the engine at `exe`. `cancel` set while it runs stops the
/// engine and returns `Err("stopped")`.
pub fn transcribe_audio(exe: &Path, audio: &[f32], cancel: Option<&AtomicBool>) -> std::result::Result<Transcript, String> {
    if audio.len() > MAX_SAMPLES {
        return Err("the audio is longer than 10 hours; split it before transcribing".into());
    }
    let work = WorkDir::new().map_err(|e| format!("no temporary folder for the speech engine: {e}"))?;
    let wav = work.path.join("audio.wav");
    let json = work.path.join("result.json");
    std::fs::write(&wav, wav_bytes(audio)).map_err(|e| format!("could not write {}: {e}", wav.display()))?;
    let args: Vec<std::ffi::OsString> = vec!["--subtitle-file".into(), wav.into_os_string(), "--out".into(), json.clone().into_os_string()];
    run_engine_cancellable(exe, &args, cancel)?;
    let raw = std::fs::read_to_string(&json).map_err(|e| format!("the speech engine wrote no result ({e})"))?;
    let mut t = parse_result(&raw, audio.len())?;
    t.source = format!("external:{}", exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "external".into()));
    Ok(t)
}

/// Run the engine with `args`, wait, and turn a failure into a readable message.
pub fn run_engine(exe: &Path, args: &[std::ffi::OsString]) -> std::result::Result<String, String> {
    run_engine_cancellable(exe, args, None)
}

/// [`run_engine`] that also stops (kills) the engine when `cancel` is set; then it returns
/// `Err("stopped")`.
pub fn run_engine_cancellable(exe: &Path, args: &[std::ffi::OsString], cancel: Option<&AtomicBool>) -> std::result::Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut cmd = Command::new(exe);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("could not start the speech engine {}: {e}", exe.display()))?;
    // Drain both pipes on their own threads: the engine logs a lot to stderr, and a full pipe
    // would block it forever.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_t = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let status = loop {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("stopped".into());
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(format!("lost the speech engine: {e}")),
        }
    };
    let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned();
    let stderr_bytes = err_t.join().unwrap_or_default();
    if status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    // The engine's own error lines (`hiba: …` / `error: …`) are the useful part; logs are noise.
    let msg: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("hiba:") || l.starts_with("error:"))
        .map(|l| l.trim_start_matches("hiba:").trim_start_matches("error:").trim())
        .collect();
    let msg = if msg.is_empty() { stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string() } else { msg.join("; ") };
    let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
    Err(if msg.is_empty() { format!("the speech engine failed (exit code {code})") } else { format!("the speech engine failed (exit code {code}): {msg}") })
}

#[derive(Deserialize)]
struct ResultFile {
    #[serde(default)]
    format_version: u32,
    #[serde(default)]
    language: String,
    #[serde(default)]
    word_timing: String,
    #[serde(default)]
    words: Vec<ResultWord>,
}

#[derive(Deserialize)]
struct ResultWord {
    text: String,
    start_ms: i64,
    end_ms: i64,
}

/// Milliseconds to ticks, saturating.
pub fn ms_to_tick(ms: i64) -> Tick {
    Tick(ms.max(0).saturating_mul(TICKS_PER_SECOND / 1000))
}

/// Ticks to milliseconds (rounded down).
pub fn tick_to_ms(t: Tick) -> i64 {
    t.0 / (TICKS_PER_SECOND / 1000)
}

/// Parse an engine result into a transcript: words in time order, non-overlapping, inside the
/// audio (`samples` at 16 kHz).
pub fn parse_result(raw: &str, samples: usize) -> std::result::Result<Transcript, String> {
    let f: ResultFile = serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(|e| format!("the speech engine's result is not valid: {e}"))?;
    if f.format_version > FORMAT_VERSION {
        return Err(format!("the speech engine speaks format version {}; this FilmCraft reads up to {FORMAT_VERSION}", f.format_version));
    }
    let audio_ms = (samples as i64).saturating_mul(1000) / 16_000;
    let confidence = if f.word_timing == "word" { 1.0 } else { ESTIMATED_CONFIDENCE };
    let mut words: Vec<Word> = Vec::with_capacity(f.words.len());
    let mut last_end = 0i64;
    for w in f.words {
        let text = w.text.trim();
        if text.is_empty() {
            continue;
        }
        let start = w.start_ms.max(last_end).min(audio_ms);
        let end = w.end_ms.max(start).min(audio_ms);
        if start >= audio_ms && audio_ms > 0 {
            break;
        }
        words.push(Word { text: text.to_string(), start: ms_to_tick(start), end: ms_to_tick(end), speaker: None, confidence });
        last_end = end;
    }
    let language = if f.language.trim().is_empty() { "hu".to_string() } else { f.language.trim().to_string() };
    let mut t = Transcript { language, source: String::new(), speakers: Vec::new(), words };
    t.normalize();
    t.check()?;
    Ok(t)
}

/// A mono 16 kHz 16-bit PCM WAV file of `audio`.
pub fn wav_bytes(audio: &[f32]) -> Vec<u8> {
    let data_len = (audio.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + audio.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36u32.saturating_add(data_len)).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
    b.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
    b.extend_from_slice(&2u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for &s in audio {
        let v = if s.is_finite() { (s.clamp(-1.0, 1.0) * 32767.0) as i16 } else { 0 };
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

/// A temporary folder removed on drop.
pub struct WorkDir {
    pub path: PathBuf,
}

impl WorkDir {
    pub fn new() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("filmcraft-speech-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_describes_mono_16k_pcm() {
        let b = wav_bytes(&[0.0, 1.0, -1.0, f32::NAN]);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([b[24], b[25], b[26], b[27]]), 16_000);
        assert_eq!(u32::from_le_bytes([b[40], b[41], b[42], b[43]]), 8);
        assert_eq!(b.len(), 44 + 8);
        assert_eq!(i16::from_le_bytes([b[46], b[47]]), 32767);
        assert_eq!(i16::from_le_bytes([b[50], b[51]]), 0);
    }

    #[test]
    fn result_words_become_ordered_ticks() {
        let raw = r#"{"format_version":1,"language":"hu","word_timing":"word",
            "words":[{"text":" Jó ","start_ms":100,"end_ms":300},{"text":"","start_ms":300,"end_ms":310},
                     {"text":"napot","start_ms":250,"end_ms":600}]}"#;
        let t = parse_result(raw, 16_000).unwrap();
        assert_eq!(t.language, "hu");
        assert_eq!(t.words.len(), 2);
        assert_eq!(t.words[0].text, "Jó");
        assert_eq!(t.words[0].start, ms_to_tick(100));
        // overlapping start is pushed past the previous word
        assert_eq!(t.words[1].start, ms_to_tick(300));
        assert_eq!(t.words[1].confidence, 1.0);
    }

    #[test]
    fn estimated_timing_lowers_confidence_and_words_stay_inside_audio() {
        let raw = r#"{"format_version":1,"word_timing":"segment-estimated",
            "words":[{"text":"a","start_ms":0,"end_ms":900},{"text":"b","start_ms":900,"end_ms":5000},{"text":"c","start_ms":5000,"end_ms":6000}]}"#;
        let t = parse_result(raw, 16_000).unwrap();
        assert_eq!(t.words.len(), 2);
        assert_eq!(t.words[1].end, ms_to_tick(1000));
        assert_eq!(t.words[0].confidence, ESTIMATED_CONFIDENCE);
    }

    #[test]
    fn newer_format_and_garbage_are_refused() {
        assert!(parse_result(r#"{"format_version":2,"words":[]}"#, 16_000).is_err());
        assert!(parse_result("not json", 16_000).is_err());
    }

    #[test]
    fn ms_tick_round_trip() {
        assert_eq!(tick_to_ms(ms_to_tick(1234)), 1234);
        assert_eq!(ms_to_tick(-5), Tick(0));
    }

    #[test]
    fn missing_engine_is_a_readable_error() {
        let e = run_engine(Path::new("this-engine-does-not-exist.exe"), &[]).unwrap_err();
        assert!(e.contains("could not start the speech engine"), "{e}");
    }

    #[cfg(windows)]
    #[test]
    fn cancel_stops_a_running_engine() {
        let cancel = AtomicBool::new(false);
        let started = std::time::Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(300));
                cancel.store(true, Ordering::Relaxed);
            });
            // a command that would run for ~20 s
            let args: Vec<std::ffi::OsString> = vec!["/c".into(), "ping -n 20 127.0.0.1".into()];
            let r = run_engine_cancellable(Path::new("cmd"), &args, Some(&cancel));
            assert_eq!(r.unwrap_err(), "stopped");
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn work_dir_is_removed() {
        let p = {
            let w = WorkDir::new().unwrap();
            assert!(w.path.is_dir());
            w.path.clone()
        };
        assert!(!p.exists());
    }
}
