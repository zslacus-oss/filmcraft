//! LIKE tools: one-step Hungarian rough cut, captions and caption fixes (Sequence ▸ LIKE).
//!
//! Built on FilmCraft's own commands so every step is an ordinary, undoable edit:
//!
//! * `like.transcribe` — transcript of the sequence's audio by the external speech engine
//!   ([`crate::external_speech`]); installs it as the session's transcriber, so Text ▸ Transcribe
//!   uses it too from then on.
//! * `like.quickCut` — rough cut in one step: filler words, then pauses (`transcript.removeFillers`,
//!   `transcript.removePauses`). Hesitation sounds the recogniser left out of the transcript
//!   ("ööö") sit in the gaps between words, so the pause pass removes them as well.
//! * `like.createCaptions` — captions from the (edited) sequence transcript, broken into lines by
//!   the engine's Hungarian rules (no article or conjunction left at a line end, 42 characters,
//!   2 lines), imported as a caption track.
//! * `like.correctCaptions` — spelling and grammar fix of a caption track by the engine's language
//!   model; captions keep their number and timing.
//! * `like.moveWordNext` / `like.moveWordPrev` — move the boundary word of the selected caption
//!   into the next / previous caption, retiming the cut at that word.
//! * `like.openEngineSettings` — the engine's own window (dictionary, model, AI provider).

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use filmcraft_project::ClipId;
use filmcraft_time::Tick;

use crate::commands::{CommandSpec, bad, f64_p, has_seq, str_p, u64_p};
use crate::external_speech::{ExternalTranscriber, WorkDir, run_engine, tick_to_ms};
use crate::{EngineError, Result, Session};

type Run = fn(&mut Session, &Value) -> Result<Value>;
type Enabled = fn(&Session) -> std::result::Result<(), String>;

const MENU: &[&str] = &["Sequence", "LIKE"];

/// Hesitation sounds removed by the rough cut: never meaningful words.
pub const HU_FILLERS: &[&str] = &["ö", "öö", "ööö", "öhm", "őő", "ő", "hmm", "hm", "mm", "mhm", "izé"];

/// Discourse words that are fillers in most spoken Hungarian but can carry meaning; removed only
/// when asked for (`"fillers": "extended"`).
pub const HU_FILLERS_EXTENDED: &[&str] = &["hát", "ugye", "szóval", "na", "tehát", "mondjuk", "gyakorlatilag"];

/// Words that must not end a caption line (articles, conjunctions).
const NO_LINE_END: &[&str] = &["a", "az", "egy", "és", "s", "de", "hogy", "vagy", "ha", "mert", "mint", "is", "sem", "meg", "nem"];

fn spec(id: &'static str, label: &'static str, params: &'static str, enabled: Enabled, run: Run) -> CommandSpec {
    CommandSpec { id, label, menu: MENU, shortcut: None, params, enabled, run, journal: true }
}

fn engine_err(e: String) -> EngineError {
    EngineError::Other(e)
}

fn has_engine(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if crate::external_speech::detect().is_some() || session_engine(s).is_some() {
        Ok(())
    } else {
        Err("the LIKE speech engine (LIKE Diktálás) is not installed".into())
    }
}

fn has_engine_and_captions(s: &Session) -> std::result::Result<(), String> {
    has_engine(s)?;
    if s.active_sequence().is_some_and(|q| q.caption_tracks.iter().any(|t| !t.captions.is_empty())) { Ok(()) } else { Err("there are no captions".into()) }
}

fn has_captions(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if s.active_sequence().is_some_and(|q| q.caption_tracks.iter().any(|t| !t.captions.is_empty())) { Ok(()) } else { Err("there are no captions".into()) }
}

fn engine_installed(_: &Session) -> std::result::Result<(), String> {
    if crate::external_speech::detect().is_some() { Ok(()) } else { Err("the LIKE speech engine (LIKE Diktálás) is not installed".into()) }
}

/// The engine executable the session already uses, if any.
fn session_engine(_s: &Session) -> Option<std::path::PathBuf> {
    crate::external_speech::detect()
}

fn engine_exe(s: &Session, cmd: &str) -> Result<std::path::PathBuf> {
    session_engine(s).ok_or_else(|| bad(cmd, "the LIKE speech engine (LIKE Diktálás) is not installed"))
}

/// Make the external engine the session's transcriber.
fn install_transcriber(s: &mut Session, cmd: &str) -> Result<()> {
    let exe = engine_exe(s, cmd)?;
    s.transcriber = Some(Arc::new(ExternalTranscriber::new(exe)));
    Ok(())
}

fn transcribe(s: &mut Session, p: &Value) -> Result<Value> {
    install_transcriber(s, "like.transcribe")?;
    let mut q = json!({"language": "hu", "diarize": false});
    if let Some(items) = p.get("items") {
        q["items"] = items.clone();
    }
    s.execute("transcript.generate", q)
}

fn ensure_transcript(s: &mut Session) -> Result<bool> {
    if crate::transcript::sequence_words(s).is_empty() {
        transcribe(s, &json!({}))?;
        return Ok(true);
    }
    Ok(false)
}

fn filler_list(p: &Value) -> Vec<String> {
    match p.get("fillers") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) if s == "extended" => HU_FILLERS.iter().chain(HU_FILLERS_EXTENDED).map(|f| f.to_string()).collect(),
        _ => HU_FILLERS.iter().map(|f| f.to_string()).collect(),
    }
}

fn quick_cut(s: &mut Session, p: &Value) -> Result<Value> {
    let transcribed = ensure_transcript(s)?;
    let fillers = filler_list(p);
    let min = f64_p(p, "minSeconds").filter(|x| x.is_finite()).unwrap_or(0.6).clamp(0.2, 10.0);
    let keep = f64_p(p, "keepSeconds").filter(|x| x.is_finite()).unwrap_or(0.12).clamp(0.0, 2.0);
    let f = s.execute("transcript.removeFillers", json!({"fillers": fillers}))?;
    let pz = s.execute("transcript.removePauses", json!({"minSeconds": min, "keepSeconds": keep}))?;
    let secs = |v: &Value| v.get("seconds").and_then(Value::as_f64).unwrap_or(0.0);
    let num = |v: &Value| v.get("removed").and_then(Value::as_u64).unwrap_or(0);
    Ok(json!({
        "transcribed": transcribed,
        "fillers": num(&f),
        "pauses": num(&pz),
        "seconds": secs(&f) + secs(&pz),
    }))
}

/// Sequence words as the engine's `--segment-words` input (milliseconds of sequence time).
fn words_json(s: &Session) -> Value {
    let words: Vec<Value> =
        crate::transcript::sequence_words(s).iter().map(|w| json!({"text": w.text, "start_ms": tick_to_ms(w.start), "end_ms": tick_to_ms(w.end)})).collect();
    json!({"words": words})
}

fn create_captions(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "like.createCaptions";
    let exe = engine_exe(s, cmd)?;
    ensure_transcript(s)?;
    let words = words_json(s);
    if words["words"].as_array().is_none_or(|a| a.is_empty()) {
        return Err(bad(cmd, "the sequence has no transcript words"));
    }
    let work = WorkDir::new().map_err(|e| engine_err(format!("no temporary folder: {e}")))?;
    let input = work.path.join("words.json");
    let out = work.path.join("Magyar felirat.srt");
    let bytes = serde_json::to_vec(&words).map_err(|e| engine_err(e.to_string()))?;
    std::fs::write(&input, bytes).map_err(|e| engine_err(format!("could not write {}: {e}", input.display())))?;
    let mut args: Vec<std::ffi::OsString> = vec!["--segment-words".into(), input.into_os_string(), "--out".into(), out.clone().into_os_string()];
    if let Some(n) = u64_p(p, "maxChars") {
        args.push("--max-chars".into());
        args.push(n.clamp(10, 80).to_string().into());
    }
    if let Some(n) = u64_p(p, "lines") {
        args.push("--max-lines".into());
        args.push(n.clamp(1, 3).to_string().into());
    }
    run_engine(&exe, &args).map_err(engine_err)?;
    let srt = std::fs::read(&out).map_err(|e| engine_err(format!("the engine wrote no captions ({e})")))?;
    let name = str_p(p, "name").unwrap_or("Magyar felirat");
    let path = out.to_string_lossy().to_string();
    crate::captions::import(s, &path, &srt, filmcraft_captions::Format::Srt, Some(name), Some("hu"))
}

/// The caption track to work on: `track` (id or "C1"…), else the track of the selected caption,
/// else the first one.
fn work_track(s: &Session, p: &Value) -> Option<filmcraft_project::TrackId> {
    if p.get("track").is_some() {
        return crate::captions::track_param(s, p);
    }
    let q = s.active_sequence()?;
    if let Some(id) = s.state.caption_selection.first()
        && let Some((t, _)) = q.find_caption(*id)
    {
        return Some(t);
    }
    q.caption_tracks.first().map(|t| t.id)
}

fn correct_captions(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "like.correctCaptions";
    let exe = engine_exe(s, cmd)?;
    let track = work_track(s, p).ok_or_else(|| bad(cmd, "no caption track"))?;
    let caps: Vec<(ClipId, String)> =
        s.active_sequence().and_then(|q| q.caption_track(track)).map(|t| t.captions.iter().map(|c| (c.id, c.text.clone())).collect()).unwrap_or_default();
    if caps.is_empty() {
        return Err(bad(cmd, "the caption track is empty"));
    }
    let input_json = json!({"captions": caps.iter().map(|(id, text)| json!({"id": id.0, "text": text.replace('\n', " ")})).collect::<Vec<_>>()});
    let work = WorkDir::new().map_err(|e| engine_err(format!("no temporary folder: {e}")))?;
    let input = work.path.join("captions.json");
    let out = work.path.join("corrected.json");
    let bytes = serde_json::to_vec(&input_json).map_err(|e| engine_err(e.to_string()))?;
    std::fs::write(&input, bytes).map_err(|e| engine_err(format!("could not write {}: {e}", input.display())))?;
    let args: Vec<std::ffi::OsString> = vec!["--correct-captions".into(), input.into_os_string(), "--out".into(), out.clone().into_os_string()];
    run_engine(&exe, &args).map_err(engine_err)?;
    let raw = std::fs::read_to_string(&out).map_err(|e| engine_err(format!("the engine wrote no result ({e})")))?;
    let v: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(|e| engine_err(format!("the engine's result is not valid: {e}")))?;
    let fixed = v.get("captions").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut changed = 0usize;
    for f in fixed {
        let (Some(id), Some(text)) = (f.get("id").and_then(Value::as_u64), f.get("text").and_then(Value::as_str)) else {
            continue;
        };
        let Some((_, old)) = caps.iter().find(|(c, _)| c.0 == id) else {
            continue;
        };
        if old.replace('\n', " ").split_whitespace().eq(text.split_whitespace()) {
            continue;
        }
        let lines = old.lines().count().max(1);
        let new_text = wrap(text, lines);
        s.execute("captions.setText", json!({"caption": id, "text": new_text}))?;
        changed += 1;
    }
    let warnings = v.get("warnings").cloned().unwrap_or(json!([]));
    Ok(json!({"changed": changed, "captions": caps.len(), "warnings": warnings}))
}

/// Break `text` into at most `lines` lines (1 or 2), near the middle, never leaving an article or a
/// conjunction at a line end.
pub fn wrap(text: &str, lines: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if lines < 2 || words.len() < 2 {
        return words.join(" ");
    }
    let total: usize = words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len() - 1;
    let mut best: Option<(usize, usize)> = None; // (score, split index)
    let mut left = 0usize;
    for i in 1..words.len() {
        left += words[i - 1].chars().count() + usize::from(i > 1);
        let last = words[i - 1].trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if NO_LINE_END.contains(&last.as_str()) {
            continue;
        }
        let right = total - left - 1;
        let score = left.abs_diff(right);
        if best.is_none_or(|(b, _)| score < b) {
            best = Some((score, i));
        }
    }
    match best {
        Some((_, i)) => format!("{}\n{}", words[..i].join(" "), words[i..].join(" ")),
        None => words.join(" "),
    }
}

/// The caption to act on: the first selected one, else the one under the playhead.
fn current_caption(s: &Session, p: &Value) -> Option<ClipId> {
    if let Some(c) = u64_p(p, "caption") {
        return Some(ClipId(c));
    }
    if let Some(c) = s.state.caption_selection.first() {
        return Some(*c);
    }
    let t = s.playhead();
    s.active_sequence().and_then(|q| q.caption_tracks.iter().find_map(|tr| tr.caption_at(t).map(|c| c.id)))
}

fn norm(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Where the cut between two captions goes when `word` (the `index`-th of `words` in a caption
/// spanning `start..end`) changes sides: the word's own time in the sequence transcript when it is
/// there, else a share of the caption by characters. `at_start`: the cut goes before the word.
fn cut_time(s: &Session, start: Tick, end: Tick, words: &[&str], index: usize, at_start: bool) -> Tick {
    let target = words.get(index).map(|w| norm(w)).unwrap_or_default();
    let seq = crate::transcript::sequence_words(s);
    let inside: Vec<_> = seq.iter().filter(|w| w.end > start && w.start < end).collect();
    let hit = if at_start { inside.iter().rev().find(|w| norm(&w.text) == target) } else { inside.iter().find(|w| norm(&w.text) == target) };
    if let Some(w) = hit {
        return if at_start { w.start } else { w.end };
    }
    let total: usize = words.iter().map(|w| w.chars().count() + 1).sum::<usize>().max(1);
    let upto = if at_start { index } else { index + 1 };
    let before: usize = words[..upto.min(words.len())].iter().map(|w| w.chars().count() + 1).sum();
    let span = (end.0 - start.0).max(0);
    Tick(start.0 + ((span as i128 * before as i128) / total as i128) as i64)
}

fn move_word(s: &mut Session, p: &Value, forward: bool) -> Result<Value> {
    let cmd = if forward { "like.moveWordNext" } else { "like.moveWordPrev" };
    let id = current_caption(s, p).ok_or_else(|| bad(cmd, "no caption (select one or put the playhead on one)"))?;
    let q = s.active_sequence().ok_or_else(|| bad(cmd, "no sequence"))?;
    let (track, _) = q.find_caption(id).ok_or_else(|| bad(cmd, "no such caption"))?;
    let tr = q.caption_track(track).ok_or_else(|| bad(cmd, "no such caption track"))?;
    let i = tr.captions.iter().position(|c| c.id == id).ok_or_else(|| bad(cmd, "no such caption"))?;
    let j = if forward { i + 1 } else { i.checked_sub(1).ok_or_else(|| bad(cmd, "there is no caption before this one"))? };
    let (a, b) = if forward { (tr.captions.get(i), tr.captions.get(j)) } else { (tr.captions.get(j), tr.captions.get(i)) };
    let (Some(a), Some(b)) = (a.cloned(), b.cloned()) else {
        return Err(bad(cmd, "there is no caption after this one"));
    };
    let a_words: Vec<&str> = a.text.split_whitespace().collect();
    let b_words: Vec<&str> = b.text.split_whitespace().collect();
    let (cut, new_a, new_b) = if forward {
        if a_words.len() < 2 {
            return Err(bad(cmd, "the caption has only one word"));
        }
        let k = a_words.len() - 1;
        let t = cut_time(s, a.start, a.end(), &a_words, k, true);
        (t, a_words[..k].join(" "), format!("{} {}", a_words[k], b_words.join(" ")))
    } else {
        if b_words.len() < 2 {
            return Err(bad(cmd, "the caption has only one word"));
        }
        let t = cut_time(s, b.start, b.end(), &b_words, 0, false);
        (t, format!("{} {}", a_words.join(" "), b_words[0]), b_words[1..].join(" "))
    };
    // Keep at least a frame on both sides; the timing commands snap to frames.
    let frame = s.sequence_rate().frame_duration();
    let cut = Tick(cut.0.clamp(a.start.0 + frame.0, (b.end().0 - frame.0).max(a.start.0 + frame.0)));
    // Shrink first, then grow, so the two never overlap in between.
    if forward {
        s.execute("captions.setTimes", json!({"caption": a.id.0, "endTime": cut.0}))?;
        s.execute("captions.setTimes", json!({"caption": b.id.0, "startTime": cut.0}))?;
    } else {
        s.execute("captions.setTimes", json!({"caption": b.id.0, "startTime": cut.0}))?;
        s.execute("captions.setTimes", json!({"caption": a.id.0, "endTime": cut.0}))?;
    }
    let lines = |t: &str| t.lines().count().max(1);
    s.execute("captions.setText", json!({"caption": a.id.0, "text": wrap(&new_a, lines(&a.text))}))?;
    s.execute("captions.setText", json!({"caption": b.id.0, "text": wrap(&new_b, lines(&b.text))}))?;
    Ok(json!({"cut": cut.0, "from": if forward { a.id.0 } else { b.id.0 }, "to": if forward { b.id.0 } else { a.id.0 }}))
}

fn open_engine_settings(s: &mut Session, _: &Value) -> Result<Value> {
    let exe = engine_exe(s, "like.openEngineSettings")?;
    std::process::Command::new(&exe).spawn().map_err(|e| engine_err(format!("could not start {}: {e}", exe.display())))?;
    Ok(json!({"started": exe.display().to_string()}))
}

/// Whether `path` is an engine executable this module can drive (used by the host's settings).
pub fn engine_path_ok(path: &Path) -> bool {
    path.is_file()
}

pub fn commands() -> Vec<CommandSpec> {
    let mut v = vec![
        spec(
            "like.quickCut",
            "Gyors durva vágás (szünetek, töltelékhangok)",
            r#"{"minSeconds":f=0.6,"keepSeconds":f=0.12,"fillers":[str]|"extended"?}"#,
            has_engine,
            quick_cut,
        ),
        spec("like.transcribe", "Magyar leirat készítése", r#"{"items":[id]?}"#, has_engine, transcribe),
        spec("like.createCaptions", "Magyar felirat készítése", r#"{"maxChars":n=42?,"lines":n=2?,"name":str?}"#, has_engine, create_captions),
        spec("like.correctCaptions", "Felirat AI-javítása (helyesírás, ragozás)", r#"{"track":id|"C1"?}"#, has_engine_and_captions, correct_captions),
        spec("like.moveWordNext", "Utolsó szó át a következő feliratba", r#"{"caption":id?}"#, has_captions, |s, p| move_word(s, p, true)),
        spec("like.moveWordPrev", "Első szó vissza az előző feliratba", r#"{"caption":id?}"#, has_captions, |s, p| move_word(s, p, false)),
    ];
    let mut settings = spec("like.openEngineSettings", "LIKE motor beállításai (szótár, modell, AI)…", "{}", engine_installed, open_engine_settings);
    settings.journal = false;
    v.push(settings);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_balances_and_respects_hungarian_line_ends() {
        assert_eq!(wrap("Ez egy rövid mondat", 1), "Ez egy rövid mondat");
        let w = wrap("Ma a stúdióban egy új videót készítünk a mesterséges intelligenciáról", 2);
        let (l1, l2) = w.split_once('\n').unwrap();
        let last = l1.split_whitespace().last().unwrap();
        assert!(!NO_LINE_END.contains(&last.to_lowercase().as_str()), "{w}");
        assert!(l1.chars().count().abs_diff(l2.chars().count()) < 20, "{w}");
    }

    #[test]
    fn wrap_never_ends_a_line_on_an_article() {
        let w = wrap("ez a ház", 2);
        assert!(!w.lines().next().unwrap().ends_with(" a"), "{w}");
    }

    #[test]
    fn wrap_handles_one_word_and_spaces() {
        assert_eq!(wrap("  szó  ", 2), "szó");
        assert_eq!(wrap("", 2), "");
    }

    #[test]
    fn filler_lists() {
        assert_eq!(filler_list(&json!({})).len(), HU_FILLERS.len());
        assert_eq!(filler_list(&json!({"fillers": "extended"})).len(), HU_FILLERS.len() + HU_FILLERS_EXTENDED.len());
        assert_eq!(filler_list(&json!({"fillers": ["x"]})), vec!["x".to_string()]);
    }

    #[test]
    fn commands_live_under_sequence_like() {
        for c in commands() {
            assert!(c.id.starts_with("like."));
            assert_eq!(c.menu, MENU);
        }
    }
}
