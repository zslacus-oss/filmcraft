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
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};

use filmcraft_project::{ClipId, ItemId, ItemKind, Transcript};
use filmcraft_time::Tick;

use crate::commands::{CommandSpec, bad, bool_p, f64_p, has_seq, str_p, u64_p};
use crate::external_speech::{ExternalTranscriber, WorkDir, run_engine, run_engine_cancellable, tick_to_ms, transcribe_audio};
use crate::{EngineError, Result, Session};

type Run = fn(&mut Session, &Value) -> Result<Value>;
type Enabled = fn(&Session) -> std::result::Result<(), String>;

pub mod repeats;

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

/// Make the external engine the session's transcriber, so Text ▸ Transcribe uses it too.
fn install_transcriber(s: &mut Session, cmd: &str) -> Result<std::path::PathBuf> {
    let exe = engine_exe(s, cmd)?;
    s.transcriber = Some(Arc::new(ExternalTranscriber::new(exe.clone())));
    Ok(exe)
}

// ---------------------------------------------------------------------------------------------
// Background jobs: the engine runs on its own thread, shown in the Progress panel (`jobs.list`),
// stopped by `jobs.cancel`; `poll` applies the result on the UI thread as one undo step.
// ---------------------------------------------------------------------------------------------

/// What to do once a transcript arrives.
#[derive(Clone, Debug)]
enum Then {
    Nothing,
    QuickCut(Value),
    Captions(Value),
}

/// The result a LIKE job hands back to the UI thread.
enum Output {
    Transcripts { results: Vec<(ItemId, Transcript)>, then: Then },
    Corrections { snapshot: Vec<(ClipId, String)>, fixed: Vec<(u64, String)>, warnings: Vec<String> },
    PostTexts { path: std::path::PathBuf },
}

struct Pending {
    progress: Arc<filmcraft_export::Progress>,
    out: Arc<Mutex<Option<Output>>>,
}

/// Jobs started by this module; matched to a session's jobs by their progress handle.
static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

type Work = Box<dyn FnOnce(&filmcraft_export::Progress) -> std::result::Result<Output, String> + Send>;

/// Start `work` as a background job (or run it now with `wait`); returns the job id.
fn start_job(s: &mut Session, label: String, total: u64, wait: bool, work: Work) -> Result<u64> {
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label, progress: Default::default(), result: Default::default() };
    job.progress.total.store(total.max(1), Ordering::Relaxed);
    let out: Arc<Mutex<Option<Output>>> = Arc::default();
    let (prog, res, o) = (job.progress.clone(), job.result.clone(), out.clone());
    let run = move || {
        let t0 = web_time::Instant::now();
        let r = work(&prog);
        let seconds = t0.elapsed().as_secs_f64();
        let report = match r {
            Ok(output) => {
                *lock(&o) = Some(output);
                *lock(&prog.status) = "Kész".into();
                Ok(filmcraft_export::Report { path: String::new(), frames: 0, seconds, bytes: 0, render_fps: 0.0, extra_files: Vec::new() })
            }
            Err(e) => {
                *lock(&prog.status) = if e == "stopped" { "Megszakítva: semmi nem változott".into() } else { e.clone() };
                Err(e)
            }
        };
        *lock(&res) = Some(report);
        prog.done.store(prog.total.load(Ordering::Relaxed), Ordering::Relaxed);
        prog.finished.store(true, Ordering::Relaxed);
    };
    let run = crate::export_tools::guard_job(job.progress.clone(), job.result.clone(), run);
    lock(&PENDING).push(Pending { progress: job.progress.clone(), out });
    s.jobs.push(job);
    if wait || cfg!(target_arch = "wasm32") {
        run();
        poll(s);
        if let Some(Err(e)) = s.jobs.iter().find(|j| j.id == id).and_then(|j| lock(&j.result).clone()) {
            return Err(EngineError::Other(e));
        }
    } else {
        std::thread::Builder::new().name("like-engine".into()).spawn(run).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    Ok(id)
}

/// Apply finished LIKE jobs of this session. Called once per UI frame from
/// [`Session::poll_persistence`] and after synchronous runs.
pub fn poll(s: &mut Session) {
    let ready: Vec<(Arc<filmcraft_export::Progress>, Option<Output>)> = {
        let mut pending = lock(&PENDING);
        let mut ready = Vec::new();
        pending.retain(|pj| {
            let mine = s.jobs.iter().any(|j| Arc::ptr_eq(&j.progress, &pj.progress));
            if !mine {
                // another session's job, unless nobody else holds it any more
                return Arc::strong_count(&pj.progress) > 1;
            }
            if !pj.progress.finished.load(Ordering::Relaxed) {
                return true;
            }
            let cancelled = pj.progress.cancel.load(Ordering::Relaxed);
            ready.push((pj.progress.clone(), lock(&pj.out).take().filter(|_| !cancelled)));
            false
        });
        ready
    };
    for (progress, out) in ready {
        match out {
            Some(o) => {
                if let Err(e) = apply(s, o) {
                    s.error_toast("like", format!("LIKE: {e}"));
                }
            }
            None => {
                let err = s.jobs.iter().find(|j| Arc::ptr_eq(&j.progress, &progress)).and_then(|j| lock(&j.result).clone());
                if let Some(Err(e)) = err
                    && e != "stopped"
                {
                    s.error_toast("like", format!("LIKE: {e}"));
                }
            }
        }
    }
}

fn apply(s: &mut Session, out: Output) -> Result<Value> {
    match out {
        Output::Transcripts { results, then } => {
            // the project may have changed meanwhile: keep only items that are still media
            let results: Vec<(ItemId, Transcript)> =
                results.into_iter().filter(|(i, _)| s.project.item(*i).is_some_and(|it| matches!(it.kind, ItemKind::Media(_)))).collect();
            if results.is_empty() {
                return Err(EngineError::Other("the transcribed clips are no longer in the project".into()));
            }
            let words: usize = results.iter().map(|(_, t)| t.words.len()).sum();
            s.edit("Magyar leirat", move |pr, _| {
                for (i, t) in results {
                    pr.transcripts.insert(i, Arc::new(t));
                }
                Ok(())
            })?;
            s.toast(format!("Magyar leirat kész: {words} szó"));
            match then {
                Then::Nothing => Ok(json!({"words": words})),
                Then::QuickCut(p) => {
                    let r = quick_cut_now(s, &p)?;
                    s.toast(quick_cut_message(&r));
                    Ok(r)
                }
                Then::Captions(p) => {
                    let r = create_captions_now(s, &p)?;
                    s.toast("Magyar felirat kész");
                    Ok(r)
                }
            }
        }
        Output::Corrections { snapshot, fixed, warnings } => {
            let mut changed = 0usize;
            for (id, text) in fixed {
                let Some((_, old)) = snapshot.iter().find(|(c, _)| c.0 == id) else { continue };
                // skip captions edited (or deleted) by hand while the model was working
                let current = s.active_sequence().and_then(|q| q.find_caption(ClipId(id))).map(|(_, c)| c.text.clone());
                if current.as_deref() != Some(old.as_str()) {
                    continue;
                }
                if old.replace('\n', " ").split_whitespace().eq(text.split_whitespace()) {
                    continue;
                }
                s.execute("captions.setText", json!({"caption": id, "text": wrap(&text, old.lines().count().max(1))}))?;
                changed += 1;
            }
            s.toast(format!("AI-javítás kész: {changed} felirat változott"));
            for w in &warnings {
                s.error_toast("like", format!("LIKE: {w}"));
            }
            Ok(json!({"changed": changed, "warnings": warnings}))
        }
        Output::PostTexts { path } => {
            s.toast(format!("Poszt-szövegek kész: {}", path.display()));
            open_document(&path);
            Ok(json!({"path": path.display().to_string()}))
        }
    }
}

/// Open a finished text file for the user (Notepad on Windows, the default app elsewhere).
fn open_document(path: &Path) {
    let r = if cfg!(windows) {
        std::process::Command::new("notepad.exe").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(path).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(path).spawn()
    };
    if let Err(e) = r {
        log::warn!("could not open {}: {e}", path.display());
    }
}

/// Where LIKE writes files the user takes away (post texts, exports): next to the saved project,
/// else `Documents/Like FilmCraft`.
fn output_dir(s: &Session) -> std::path::PathBuf {
    if let Some(dir) = s.path.as_deref().and_then(|p| Path::new(p).parent()).filter(|d| d.is_dir()) {
        return dir.to_path_buf();
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = home.join("Documents").join("Like FilmCraft");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A file name made from the active sequence's name (characters Windows refuses replaced).
fn sequence_file_stem(s: &Session) -> String {
    let name = s.state.active_sequence.and_then(|id| s.project.item(id)).map(|it| it.name.clone()).unwrap_or_else(|| "Video".into());
    let clean: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() { "Video".into() } else { clean }
}

fn post_texts(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "like.postTexts";
    let exe = engine_exe(s, cmd)?;
    let text = crate::transcript::sequence_words(s).iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    if text.trim().is_empty() {
        return Err(bad(cmd, "the sequence has no transcript (Magyar leirat készítése first)"));
    }
    let platforms = str_p(p, "platforms").unwrap_or("youtube,tiktok,instagram,linkedin").to_string();
    let out = output_dir(s).join(format!("{} – poszt-szövegek.md", sequence_file_stem(s)));
    let wait = bool_p(p, "wait").unwrap_or(false);
    let job = start_job(
        s,
        "Poszt-szöveg és hashtagek (AI)".into(),
        1,
        wait,
        Box::new(move |prog: &filmcraft_export::Progress| {
            *lock(&prog.status) = "Poszt-szövegek írása…".into();
            let work = WorkDir::new().map_err(|e| format!("no temporary folder: {e}"))?;
            let input = work.path.join("leirat.txt");
            std::fs::write(&input, text.as_bytes()).map_err(|e| format!("could not write {}: {e}", input.display()))?;
            let args: Vec<std::ffi::OsString> =
                vec!["--repurpose".into(), input.into_os_string(), "--platforms".into(), platforms.into(), "--out".into(), out.clone().into_os_string()];
            run_engine_cancellable(&exe, &args, Some(&prog.cancel))?;
            if !out.is_file() {
                return Err("the engine wrote no post texts".into());
            }
            Ok(Output::PostTexts { path: out })
        }),
    )?;
    Ok(json!({"job": job, "background": !wait}))
}

// ---------------------------------------------------------------------------------------------
// One-click audio clean-up and platform exports, on FilmCraft's own Essential Sound and export.
// ---------------------------------------------------------------------------------------------

/// Mains hum in Hungary (and the rest of Europe).
const MAINS_HUM_HZ: u32 = 50;

fn audio_clips(s: &Session) -> Vec<u64> {
    s.active_sequence().map(|q| q.audio_tracks.iter().flat_map(|t| t.items.iter()).map(|it| it.id.0).collect()).unwrap_or_default()
}

fn has_audio_clips(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if audio_clips(s).is_empty() { Err("the sequence has no audio clips".into()) } else { Ok(()) }
}

/// Dialogue clean-up of every audio clip: the built-in "Clean Up Noisy Dialogue" preset (noise,
/// rumble, hum, dynamics), hum at 50 Hz, then loudness matched.
fn clean_audio(s: &mut Session, p: &Value) -> Result<Value> {
    let clips = match p.get("clips").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_u64).collect(),
        None => audio_clips(s),
    };
    if clips.is_empty() {
        return Err(bad("like.cleanAudio", "the sequence has no audio clips"));
    }
    s.execute("essentialSound.setType", json!({"clips": clips, "type": "dialogue"}))?;
    s.execute("essentialSound.applyPreset", json!({"clips": clips, "preset": "Clean Up Noisy Dialogue", "type": "dialogue"}))?;
    s.execute("essentialSound.set", json!({"clips": clips, "key": "repair.humHz", "value": MAINS_HUM_HZ}))?;
    let matched = s.execute("essentialSound.autoMatch", json!({"clips": clips}))?;
    s.toast(format!("Hangtisztítás kész: {} klip (zajszűrés, zúgásszűrés {MAINS_HUM_HZ} Hz, hangosság)", clips.len()));
    Ok(json!({"clips": clips.len(), "autoMatch": matched}))
}

/// A platform export: (file suffix, built-in preset, overridden frame size).
fn platform_export(platform: &str) -> Option<(&'static str, &'static str, Option<(u32, u32)>)> {
    Some(match platform {
        "youtube" => ("YouTube 16x9", "YouTube 1080p Full HD", None),
        "vertical" => ("TikTok-Reels-Shorts 9x16", "TikTok 1080×1920", None),
        "square" => ("LinkedIn 1x1", "Social Vertical 1080×1920", Some((1080, 1080))),
        "portrait" => ("LinkedIn-Instagram 4x5", "Social Vertical 1080×1920", Some((1080, 1350))),
        _ => return None,
    })
}

fn export_for(s: &mut Session, platform: &str, p: &Value) -> Result<Value> {
    let cmd = "like.export";
    let (suffix, preset, size) = platform_export(platform).ok_or_else(|| bad(cmd, format!("unknown platform `{platform}`")))?;
    let has_captions = s.active_sequence().is_some_and(|q| q.caption_tracks.iter().any(|t| t.enabled && !t.captions.is_empty()));
    let path = output_dir(s).join(format!("{} – {suffix}.mp4", sequence_file_stem(s)));
    let mut q = json!({
        "path": path.to_string_lossy(),
        "preset": preset,
        "burnCaptions": bool_p(p, "burnCaptions").unwrap_or(has_captions),
        "loudnessLufs": -14.0,
        "wait": bool_p(p, "wait").unwrap_or(false),
    });
    if let Some((w, h)) = size {
        q["width"] = json!(w);
        q["height"] = json!(h);
    }
    // optional range (In/Out, work area, or seconds) passed straight to file.exportMedia
    for key in ["range", "startSeconds", "endSeconds"] {
        if let Some(v) = p.get(key) {
            q[key] = v.clone();
        }
    }
    let r = s.execute("file.exportMedia", q)?;
    s.toast(format!("Exportálás elindult: {}", path.display()));
    Ok(json!({"path": path.display().to_string(), "export": r}))
}

fn export_all(s: &mut Session, p: &Value) -> Result<Value> {
    let mut out = Vec::new();
    for platform in ["youtube", "vertical", "square", "portrait"] {
        out.push(export_for(s, platform, p)?);
    }
    Ok(json!({"exports": out}))
}

/// Media items to transcribe: `items`, else the media of the active sequence's enabled audio clips.
fn transcribe_targets(s: &Session, p: &Value) -> Vec<ItemId> {
    let raw: Vec<ItemId> = match p.get("items").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_u64).map(ItemId).collect(),
        None => s
            .active_sequence()
            .map(|q| q.audio_tracks.iter().flat_map(|t| t.items.iter()).filter(|it| it.enabled).map(|it| it.item).collect())
            .unwrap_or_default(),
    };
    let mut out = Vec::new();
    for i in raw {
        if let Some((root, _, _)) = s.project.resolve_media(i)
            && !out.contains(&root)
        {
            out.push(root);
        }
    }
    out
}

fn start_transcribe(s: &mut Session, p: &Value, then: Then) -> Result<Value> {
    let cmd = "like.transcribe";
    let exe = install_transcriber(s, cmd)?;
    let mut work = Vec::new();
    for item in transcribe_targets(s, p) {
        let Some(it) = s.project.item(item) else { continue };
        let ItemKind::Media(m) = &it.kind else { continue };
        let len = m.duration().to_units_floor(filmcraft_speech::SAMPLE_RATE as i64).max(0) as usize;
        let name = it.name.clone();
        let Some(src) = s.source(item) else { continue };
        if !src.info().has_audio() || len == 0 {
            continue;
        }
        work.push((item, name, src, len));
    }
    if work.is_empty() {
        return Err(bad(cmd, "nothing to transcribe (open a sequence with audio clips)"));
    }
    let n = work.len();
    let label = match work.first() {
        Some((_, name, _, _)) if n == 1 => format!("Magyar leirat: {name}"),
        _ => format!("Magyar leirat ({n} klip)"),
    };
    let wait = bool_p(p, "wait").unwrap_or(false);
    let job = start_job(
        s,
        label,
        n as u64,
        wait,
        Box::new(move |prog: &filmcraft_export::Progress| {
            let mut results = Vec::new();
            for (k, (item, name, src, len)) in work.into_iter().enumerate() {
                if prog.cancel.load(Ordering::Relaxed) {
                    return Err("stopped".into());
                }
                *lock(&prog.status) = format!("{name}: hang előkészítése");
                let buf = src.audio(0, len, filmcraft_speech::SAMPLE_RATE).map_err(|e| format!("{name}: {e}"))?;
                let audio = filmcraft_speech::downmix(&buf.channels);
                *lock(&prog.status) = format!("{name}: magyar leirat készül ({}/{n})", k + 1);
                let mut t = transcribe_audio(&exe, &audio, Some(&prog.cancel))?;
                t.normalize();
                results.push((item, t));
                prog.done.store(k as u64 + 1, Ordering::Relaxed);
            }
            Ok(Output::Transcripts { results, then })
        }),
    )?;
    Ok(json!({"job": job, "clips": n, "background": !wait}))
}

fn transcribe(s: &mut Session, p: &Value) -> Result<Value> {
    start_transcribe(s, p, Then::Nothing)
}

fn has_sequence_transcript(s: &Session) -> bool {
    !crate::transcript::sequence_words(s).is_empty()
}

fn filler_list(p: &Value) -> Vec<String> {
    match p.get("fillers") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) if s == "extended" => HU_FILLERS.iter().chain(HU_FILLERS_EXTENDED).map(|f| f.to_string()).collect(),
        _ => HU_FILLERS.iter().map(|f| f.to_string()).collect(),
    }
}

fn quick_cut(s: &mut Session, p: &Value) -> Result<Value> {
    if !has_sequence_transcript(s) || bool_p(p, "retranscribe").unwrap_or(false) {
        // transcribe first (in the background); the cut follows when the transcript arrives
        return start_transcribe(s, p, Then::QuickCut(p.clone()));
    }
    quick_cut_now(s, p)
}

fn quick_cut_now(s: &mut Session, p: &Value) -> Result<Value> {
    let fillers = filler_list(p);
    let min = f64_p(p, "minSeconds").filter(|x| x.is_finite()).unwrap_or(0.6).clamp(0.2, 10.0);
    let keep = f64_p(p, "keepSeconds").filter(|x| x.is_finite()).unwrap_or(0.12).clamp(0.0, 2.0);
    let f = s.execute("transcript.removeFillers", json!({"fillers": fillers}))?;
    let pz = s.execute("transcript.removePauses", json!({"minSeconds": min, "keepSeconds": keep}))?;
    let secs = |v: &Value| v.get("seconds").and_then(Value::as_f64).unwrap_or(0.0);
    let num = |v: &Value| v.get("removed").and_then(Value::as_u64).unwrap_or(0);
    Ok(json!({"fillers": num(&f), "pauses": num(&pz), "seconds": secs(&f) + secs(&pz)}))
}

fn quick_cut_message(r: &Value) -> String {
    let n = |k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0);
    // Hungarian decimal comma
    let secs = format!("{:.1}", r.get("seconds").and_then(Value::as_f64).unwrap_or(0.0)).replace('.', ",");
    format!("Durva vágás kész: {} töltelékszó és {} szünet kivágva ({secs} mp)", n("fillers"), n("pauses"))
}

/// Sequence words as the engine's `--segment-words` input (milliseconds of sequence time).
fn words_json(s: &Session) -> Value {
    let words: Vec<Value> =
        crate::transcript::sequence_words(s).iter().map(|w| json!({"text": w.text, "start_ms": tick_to_ms(w.start), "end_ms": tick_to_ms(w.end)})).collect();
    json!({"words": words})
}

fn create_captions(s: &mut Session, p: &Value) -> Result<Value> {
    engine_exe(s, "like.createCaptions")?;
    if !has_sequence_transcript(s) {
        return start_transcribe(s, p, Then::Captions(p.clone()));
    }
    create_captions_now(s, p)
}

fn create_captions_now(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "like.createCaptions";
    let exe = engine_exe(s, cmd)?;
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
    let snapshot: Vec<(ClipId, String)> =
        s.active_sequence().and_then(|q| q.caption_track(track)).map(|t| t.captions.iter().map(|c| (c.id, c.text.clone())).collect()).unwrap_or_default();
    if snapshot.is_empty() {
        return Err(bad(cmd, "the caption track is empty"));
    }
    let input_json = json!({"captions": snapshot.iter().map(|(id, text)| json!({"id": id.0, "text": text.replace('\n', " ")})).collect::<Vec<_>>()});
    let n = snapshot.len();
    let wait = bool_p(p, "wait").unwrap_or(false);
    let job = start_job(
        s,
        format!("Felirat AI-javítása ({n} felirat)"),
        1,
        wait,
        Box::new(move |prog: &filmcraft_export::Progress| {
            *lock(&prog.status) = format!("{n} felirat javítása…");
            let work = WorkDir::new().map_err(|e| format!("no temporary folder: {e}"))?;
            let input = work.path.join("captions.json");
            let out = work.path.join("corrected.json");
            let bytes = serde_json::to_vec(&input_json).map_err(|e| e.to_string())?;
            std::fs::write(&input, bytes).map_err(|e| format!("could not write {}: {e}", input.display()))?;
            let args: Vec<std::ffi::OsString> = vec!["--correct-captions".into(), input.into_os_string(), "--out".into(), out.clone().into_os_string()];
            run_engine_cancellable(&exe, &args, Some(&prog.cancel))?;
            let raw = std::fs::read_to_string(&out).map_err(|e| format!("the engine wrote no result ({e})"))?;
            let v: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(|e| format!("the engine's result is not valid: {e}"))?;
            let fixed: Vec<(u64, String)> = v
                .get("captions")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|f| Some((f.get("id")?.as_u64()?, f.get("text")?.as_str()?.to_string()))).collect())
                .unwrap_or_default();
            let warnings: Vec<String> =
                v.get("warnings").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
            Ok(Output::Corrections { snapshot, fixed, warnings })
        }),
    )?;
    Ok(json!({"job": job, "captions": n, "background": !wait}))
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

// ---------------------------------------------------------------------------------------------
// Retakes: the earlier attempts are marked for review (red range markers); the user deletes the
// markers of takes to keep, then cuts the rest. The last reading is always kept.
// ---------------------------------------------------------------------------------------------

/// Name of the review markers; `like.cutMarkedRetakes` cuts exactly these.
pub const RETAKE_MARKER: &str = "Ismételt take – kivágásra javasolt";
const RETAKE_COLOR: &str = "#e53935";

fn has_transcript(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if has_sequence_transcript(s) { Ok(()) } else { Err("the sequence has no transcript (Magyar leirat készítése first)".into()) }
}

fn retake_markers(s: &Session) -> Vec<(filmcraft_project::MarkerId, Tick, Tick)> {
    s.active_sequence()
        .map(|q| q.markers.iter().filter(|m| m.name == RETAKE_MARKER && m.duration > Tick::ZERO).map(|m| (m.id, m.start, m.duration)).collect())
        .unwrap_or_default()
}

fn has_retake_markers(s: &Session) -> std::result::Result<(), String> {
    has_seq(s)?;
    if retake_markers(s).is_empty() { Err("there are no retake markers (Ismételt mondatok megjelölése first)".into()) } else { Ok(()) }
}

/// Spoken edit cues: what the speaker says to mark a mistake ("ezt vágjuk ki", "elrontottam").
const SPOKEN_CUES: &[&str] = &[
    "ezt vágjuk ki",
    "ezt vágd ki",
    "ezt kivágjuk",
    "ezt ki kell vágni",
    "ki kell vágni",
    "vágjuk ki",
    "elrontottam",
    "ez egy hiba volt",
    "ez hiba volt",
    "újrakezdem",
    "kezdem újra",
    "ezt újra mondom",
];

/// Longest stretch, in words, a cue reaches back for the clause it refers to.
const CUE_REACH: usize = 15;

/// Word ranges (`from..to`, inclusive end excluded) a spoken cue asks to cut: the clause before
/// the cue (back to the previous comma or sentence end, at most [`CUE_REACH`] words) and the cue
/// itself. Overlapping cues give overlapping ranges; the cut merges them.
pub fn find_spoken_cuts(words: &[filmcraft_edit::transcript::SeqWord]) -> Vec<(std::ops::Range<usize>, String)> {
    let norm: Vec<String> = words.iter().map(|w| repeats::normalize(&w.text)).collect();
    let cues: Vec<Vec<&str>> = SPOKEN_CUES.iter().map(|c| c.split_whitespace().collect()).collect();
    let ends_clause = |w: &str| w.trim_end_matches(['"', '\'', ')', '\u{201d}']).ends_with([',', '.', '!', '?', ';', ':', '…']);
    let mut out: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut i = 0;
    while i < norm.len() {
        // the longest cue that starts here
        let hit = cues.iter().filter(|c| norm.get(i..i + c.len()).is_some_and(|w| w.iter().zip(c.iter()).all(|(a, b)| a == b))).map(|c| c.len()).max();
        let Some(len) = hit else {
            i += 1;
            continue;
        };
        let end = i + len;
        // back to the start of the clause before the cue; the word right before the cue may
        // itself end the clause ("elrontottam, ezt vágjuk ki")
        let mut start = i;
        let mut k = i;
        while k > 0 && i - k < CUE_REACH {
            let prev = &words[k - 1].text;
            let sentence_end = prev.trim_end_matches(['"', '\'', ')', '\u{201d}']).ends_with(['.', '!', '?', '…']);
            // a comma right before the cue belongs to the mistake ("elrontottam, ezt vágjuk ki");
            // a sentence end never does ("Ez jó volt. Elrontottam.")
            if (k < i && ends_clause(prev)) || (k == i && sentence_end) {
                break;
            }
            k -= 1;
            start = k;
        }
        let cue: Vec<&str> = words[i..end].iter().map(|w| w.text.as_str()).collect();
        let cue = cue.join(" ");
        match out.last_mut() {
            // cues side by side ("elrontottam, ezt vágjuk ki") are one cut
            Some(last) if last.0.end >= i => {
                last.0.end = end;
                last.1 = cue;
            }
            _ => out.push((start..end, cue)),
        }
        i = end;
    }
    out
}

fn mark_retakes(s: &mut Session, _p: &Value) -> Result<Value> {
    let words = crate::transcript::sequence_words(s);
    let hits = repeats::find_repeats(&words, &repeats::RepeatOptions::default());
    let spoken = find_spoken_cuts(&words);
    let rate = s.sequence_rate();
    // markers of an earlier run are replaced, not doubled
    let old: Vec<u64> = retake_markers(s).iter().map(|(id, _, _)| id.0).collect();
    if !old.is_empty() {
        s.execute("markers.delete", json!({"markers": old}))?;
    }
    let mut marked = 0usize;
    for h in &hits {
        let to = h.cut.end.saturating_sub(1).max(h.cut.start);
        let Some(r) = filmcraft_edit::transcript::word_range(&words, h.cut.start, to, rate) else { continue };
        let earlier: Vec<&str> = words.get(h.cut.clone()).unwrap_or_default().iter().take(30).map(|w| w.text.as_str()).collect();
        let later: Vec<&str> = words.get(h.later..(h.later + 12).min(words.len())).unwrap_or_default().iter().map(|w| w.text.as_str()).collect();
        let comment = format!("Elhagyott próbálkozás: „{}”\nMegtartott változat: „{} …”", earlier.join(" "), later.join(" "));
        s.execute(
            "markers.addRange",
            json!({"time": r.start.0, "duration": r.duration.0, "name": RETAKE_MARKER, "comment": comment, "color": RETAKE_COLOR, "author": "LIKE"}),
        )?;
        marked += 1;
    }
    let mut cues = 0usize;
    for (range, cue) in &spoken {
        let Some(r) = filmcraft_edit::transcript::word_range(&words, range.start, range.end.saturating_sub(1).max(range.start), rate) else { continue };
        let said: Vec<&str> = words.get(range.clone()).unwrap_or_default().iter().take(30).map(|w| w.text.as_str()).collect();
        let comment = format!("Szóbeli vágási jel („{cue}”): „{}”", said.join(" "));
        s.execute(
            "markers.addRange",
            json!({"time": r.start.0, "duration": r.duration.0, "name": RETAKE_MARKER, "comment": comment, "color": RETAKE_COLOR, "author": "LIKE"}),
        )?;
        cues += 1;
    }
    s.toast(if marked + cues == 0 {
        "Nem találtam ismételt mondatot vagy szóbeli vágási jelet".to_string()
    } else {
        format!(
            "{marked} ismételt próbálkozás és {cues} szóbeli vágási jel megjelölve pirossal. Amelyiket meg akarod tartani, annak töröld a jelölőjét, majd: Megjelölt ismétlések kivágása"
        )
    });
    Ok(json!({"marked": marked, "spokenCues": cues}))
}

/// Overlapping or touching ranges merged, in time order.
fn merge_ranges(mut ranges: Vec<filmcraft_time::TimeRange>) -> Vec<filmcraft_time::TimeRange> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<filmcraft_time::TimeRange> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.start + last.duration => {
                let end = (last.start + last.duration).max(r.start + r.duration);
                last.duration = end - last.start;
            }
            _ => out.push(r),
        }
    }
    out
}

fn cut_marked_retakes(s: &mut Session, _p: &Value) -> Result<Value> {
    let marks = retake_markers(s);
    if marks.is_empty() {
        return Err(bad("like.cutMarkedRetakes", "there are no retake markers"));
    }
    let ids: Vec<u64> = marks.iter().map(|(id, _, _)| id.0).collect();
    let ranges = merge_ranges(marks.iter().map(|(_, start, dur)| filmcraft_time::TimeRange { start: *start, duration: *dur }).collect());
    let n = ranges.len();
    // the markers go first: they sit on the ranges that disappear
    s.execute("markers.delete", json!({"markers": ids}))?;
    let total = s.edit_sequence("Ismétlések kivágása", |q, ctx, _| Ok(filmcraft_edit::transcript::ripple_delete_ranges(q, ranges, ctx)))?;
    let secs = format!("{:.1}", total.0 as f64 / filmcraft_time::TICKS_PER_SECOND as f64).replace('.', ",");
    s.toast(format!("{n} ismételt próbálkozás kivágva ({secs} mp)"));
    Ok(json!({"removed": n, "ticks": total.0}))
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
        spec("like.markRetakes", "Ismételt mondatok megjelölése", "{}", has_transcript, mark_retakes),
        spec("like.cutMarkedRetakes", "Megjelölt ismétlések kivágása", "{}", has_retake_markers, cut_marked_retakes),
        spec("like.cleanAudio", "Hangtisztítás (zaj, zúgás, hangosság)", r#"{"clips":[id]?}"#, has_audio_clips, clean_audio),
        spec("like.postTexts", "Poszt-szöveg és hashtagek (AI)", r#"{"platforms":"youtube,tiktok,instagram,linkedin"?}"#, has_transcript, post_texts),
        spec("like.export.youtube", "Export: YouTube (16:9)", r#"{"burnCaptions":bool?}"#, has_seq, |s, p| export_for(s, "youtube", p)),
        spec("like.export.vertical", "Export: TikTok / Reels / Shorts (9:16)", r#"{"burnCaptions":bool?}"#, has_seq, |s, p| export_for(s, "vertical", p)),
        spec("like.export.square", "Export: LinkedIn (1:1)", r#"{"burnCaptions":bool?}"#, has_seq, |s, p| export_for(s, "square", p)),
        spec("like.export.portrait", "Export: LinkedIn / Instagram (4:5)", r#"{"burnCaptions":bool?}"#, has_seq, |s, p| export_for(s, "portrait", p)),
        spec("like.export.all", "Export: mind a négy platformra", r#"{"burnCaptions":bool?}"#, has_seq, export_all),
    ];
    // the ported detector's own commands (agents, tests); not in menus
    v.extend(repeats::commands());
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

    fn spoken(text: &str) -> Vec<filmcraft_edit::transcript::SeqWord> {
        text.split_whitespace()
            .enumerate()
            .map(|(k, w)| filmcraft_edit::transcript::SeqWord {
                text: w.into(),
                start: Tick::from_seconds_f64(k as f64 * 0.4),
                end: Tick::from_seconds_f64(k as f64 * 0.4 + 0.3),
                clip: ClipId(1),
                item: ItemId(1),
                index: k,
                track: 0,
                speaker: None,
                confidence: 1.0,
            })
            .collect()
    }

    fn cut_words(w: &[filmcraft_edit::transcript::SeqWord], r: &std::ops::Range<usize>) -> String {
        w[r.clone()].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    /// Élő próba (2026-11-04) szövegével.
    #[test]
    fn spoken_cues_mark_the_clause_before_them() {
        let w = spoken("de még tudok mondani sokat, és most elrontottam, ezt vágjuk ki most sokat tud nekünk segíteni.");
        let cuts = find_spoken_cuts(&w);
        assert_eq!(cuts.len(), 1, "{cuts:?}");
        assert_eq!(cut_words(&w, &cuts[0].0), "és most elrontottam, ezt vágjuk ki");
        assert_eq!(cuts[0].1, "ezt vágjuk ki");
    }

    #[test]
    fn a_cue_alone_and_the_longest_cue_win() {
        let w = spoken("Ez jó volt. Elrontottam. Folytatom.");
        let cuts = find_spoken_cuts(&w);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cut_words(&w, &cuts[0].0), "Elrontottam.");
        // "ezt ki kell vágni" is one cue, not "ki kell vágni" after a stray "ezt"
        let w = spoken("tele van hibával, ezt ki kell vágni oké");
        let cuts = find_spoken_cuts(&w);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].1, "ezt ki kell vágni");
        // the clause before the cue is the mistake it points at
        assert_eq!(cut_words(&w, &cuts[0].0), "tele van hibával, ezt ki kell vágni");
    }

    #[test]
    fn ordinary_speech_has_no_cues() {
        assert!(find_spoken_cuts(&spoken("Ma a mesterséges intelligenciáról beszélek, és arról, hogyan segít.")).is_empty());
    }

    #[test]
    fn overlapping_ranges_merge() {
        let r = |a: i64, d: i64| filmcraft_time::TimeRange { start: Tick(a), duration: Tick(d) };
        let m = merge_ranges(vec![r(50, 10), r(0, 10), r(5, 10), r(15, 5)]);
        assert_eq!(m, vec![r(0, 20), r(50, 10)]);
    }

    #[test]
    fn platform_exports_cover_the_four_formats() {
        assert_eq!(platform_export("youtube").map(|p| p.2), Some(None));
        assert_eq!(platform_export("square").map(|p| p.2), Some(Some((1080, 1080))));
        assert_eq!(platform_export("portrait").map(|p| p.2), Some(Some((1080, 1350))));
        assert!(platform_export("vertical").is_some());
        assert!(platform_export("myspace").is_none());
    }

    #[test]
    fn exports_and_cleanup_run_on_the_demo_project() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        // one-click audio clean-up
        let r = s.execute("like.cleanAudio", json!({})).unwrap();
        assert!(r["clips"].as_u64().unwrap() > 0, "{r}");
        let q = s.active_sequence().unwrap();
        let st = q.audio_tracks.iter().flat_map(|t| t.items.iter()).find_map(|it| it.essential.clone()).unwrap();
        // the preset's repairs are on (its name clears once hum is set: a custom setting now)
        assert!(st.repair.noise.on && st.repair.rumble.on && st.repair.dehum.on, "{:?}", st.repair);
        assert_eq!(st.repair.hum_hz, MAINS_HUM_HZ);
        // a 1:1 export of a short range, written to a temp folder
        let dir = std::env::temp_dir().join(format!("like-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        s.path = Some(dir.join("teszt.fcproj").to_string_lossy().into_owned());
        let e = s.execute("like.export.square", json!({"wait": true, "burnCaptions": false, "range": "custom", "startSeconds": 0, "endSeconds": 0.5})).unwrap();
        let path = std::path::PathBuf::from(e["path"].as_str().unwrap());
        assert!(path.starts_with(&dir), "{e}");
        assert!(path.to_string_lossy().ends_with("LinkedIn 1x1.mp4"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_stems_are_safe_on_windows() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let stem = sequence_file_stem(&s);
        assert!(!stem.is_empty());
        assert!(!stem.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']), "{stem}");
    }

    #[test]
    fn quick_cut_message_reads_well() {
        let m = quick_cut_message(&json!({"fillers": 3, "pauses": 12, "seconds": 41.26}));
        assert_eq!(m, "Durva vágás kész: 3 töltelékszó és 12 szünet kivágva (41,3 mp)");
    }

    #[test]
    fn commands_live_under_sequence_like() {
        for c in commands().into_iter().filter(|c| c.id.starts_with("like.")) {
            assert_eq!(c.menu, MENU, "{}", c.id);
        }
        // the ported detector's commands stay out of the menus
        for c in commands().into_iter().filter(|c| !c.id.starts_with("like.")) {
            assert!(c.menu.is_empty(), "{}", c.id);
        }
    }
}
