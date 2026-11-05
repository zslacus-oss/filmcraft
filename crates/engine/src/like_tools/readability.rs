//! Readable caption timing (Sequence ▸ LIKE ▸ Túl gyors feliratok javítása).
//!
//! A caption that leaves the screen before it can be read (more than [`MAX_CPS`] characters per
//! second, or shorter than [`MIN_SECONDS`]) is lengthened into the silence around it: first its
//! end moves later, up to [`GAP_FRAMES`] frames before the next caption, then its start moves
//! earlier, up to the same gap after the previous one. Speech timing is never overlapped, and no
//! caption grows past [`MAX_SECONDS`]. Captions that still cannot be read in time are reported, so
//! the user can shorten or merge them.

use filmcraft_time::{FrameRate, Tick};

/// Comfortable reading speed for subtitles (characters per second, spaces included).
pub const MAX_CPS: f64 = 17.0;
/// Shortest time any caption stays on screen.
pub const MIN_SECONDS: f64 = 1.0;
/// Longest time a lengthened caption may stay on screen.
pub const MAX_SECONDS: f64 = 7.0;
/// Frames left free between two captions, so the eye sees the change.
pub const GAP_FRAMES: i64 = 2;

/// One caption as the timing sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub start: Tick,
    pub end: Tick,
    pub chars: usize,
}

/// Characters a reader counts: line breaks read as spaces.
pub fn chars(text: &str) -> usize {
    text.split_whitespace().map(|w| w.chars().count() + 1).sum::<usize>().saturating_sub(1)
}

/// Time the caption needs on screen.
pub fn needed(chars: usize) -> Tick {
    Tick::from_seconds_f64((chars as f64 / MAX_CPS).clamp(MIN_SECONDS, MAX_SECONDS))
}

/// Whether a caption is too fast to read.
pub fn too_fast(b: &Block, rate: FrameRate) -> bool {
    // one frame of tolerance: times sit on frame boundaries
    b.end.0.saturating_sub(b.start.0).saturating_add(rate.frame_duration().0) < needed(b.chars).0
}

fn snap_up(rate: FrameRate, t: Tick) -> Tick {
    let down = rate.snap(t);
    if down >= t { down } else { rate.tick_of(rate.frame_at(t).saturating_add(1)) }
}

/// New (start, end) for every block (in time order), and the indexes still too fast to read.
/// `limit` is the end of the sequence: the last caption does not run past it.
pub fn readable_times(blocks: &[Block], rate: FrameRate, limit: Option<Tick>) -> (Vec<(Tick, Tick)>, Vec<usize>) {
    let frame = rate.frame_duration();
    let gap = Tick(frame.0.saturating_mul(GAP_FRAMES));
    let mut out: Vec<(Tick, Tick)> = blocks.iter().map(|b| (b.start, b.end)).collect();
    let mut still = Vec::new();
    for i in 0..blocks.len() {
        let Some(b) = blocks.get(i) else { continue };
        let Some(&(start, end)) = out.get(i) else { continue };
        let need = needed(b.chars);
        if !too_fast(&Block { start, end, chars: b.chars }, rate) {
            continue;
        }
        // later: into the silence before the next caption (its start has not moved yet)
        let end_limit = match blocks.get(i + 1) {
            Some(next) => Tick(next.start.0.saturating_sub(gap.0)),
            None => limit.unwrap_or(Tick(start.0.saturating_add(need.0))),
        };
        let want_end = Tick(start.0.saturating_add(need.0).min(end_limit.0));
        let new_end = rate.snap(want_end).max(end);
        // earlier: into the silence after the previous caption (as it is now)
        let start_limit = match i.checked_sub(1).and_then(|p| out.get(p)) {
            Some(&(_, prev_end)) => Tick(prev_end.0.saturating_add(gap.0)),
            None => Tick(0),
        };
        let want_start = Tick(new_end.0.saturating_sub(need.0).max(start_limit.0));
        let new_start = snap_up(rate, want_start).min(start);
        if let Some(slot) = out.get_mut(i) {
            *slot = (new_start, new_end);
        }
        if too_fast(&Block { start: new_start, end: new_end, chars: b.chars }, rate) {
            still.push(i);
        }
    }
    (out, still)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: f64) -> Tick {
        Tick::from_seconds_f64(x)
    }

    fn rate() -> FrameRate {
        FrameRate { num: 25, den: 1 }
    }

    #[test]
    fn counts_characters_like_a_reader() {
        assert_eq!(chars("Ma arról\nbeszélek"), "Ma arról beszélek".chars().count());
        assert_eq!(chars("  "), 0);
        assert_eq!(chars("őű"), 2);
    }

    #[test]
    fn a_fast_caption_grows_into_the_silence_after_it() {
        // 34 characters in 1 s, 3 s of silence after it: needs 2 s
        let b = [Block { start: s(0.0), end: s(1.0), chars: 34 }, Block { start: s(4.0), end: s(6.0), chars: 10 }];
        let (t, still) = readable_times(&b, rate(), None);
        assert_eq!(t[0], (s(0.0), s(2.0)));
        assert_eq!(t[1], (s(4.0), s(6.0)), "a readable caption is left alone");
        assert!(still.is_empty());
    }

    #[test]
    fn then_into_the_silence_before_it_and_never_overlapping() {
        // the next caption follows at once: the start moves earlier instead
        let b =
            [Block { start: s(0.0), end: s(1.0), chars: 10 }, Block { start: s(3.0), end: s(4.0), chars: 34 }, Block { start: s(4.0), end: s(6.0), chars: 10 }];
        let (t, still) = readable_times(&b, rate(), None);
        let gap = rate().frame_duration().0 * GAP_FRAMES;
        assert_eq!(t[1].1, s(4.0), "an end already at the next caption stays there");
        assert!(t[1].0 < s(3.0) && t[1].0 >= t[0].1 + Tick(gap), "{t:?}");
        assert!(still.is_empty(), "{t:?}");
        for w in t.windows(2) {
            assert!(w[0].1 <= w[1].0, "{t:?}");
        }
    }

    #[test]
    fn a_caption_with_no_room_is_reported() {
        let b =
            [Block { start: s(0.0), end: s(2.0), chars: 10 }, Block { start: s(2.0), end: s(2.5), chars: 60 }, Block { start: s(2.5), end: s(5.0), chars: 10 }];
        let (t, still) = readable_times(&b, rate(), None);
        assert_eq!(still, vec![1]);
        assert!(t[1].0 <= s(2.0) && t[1].1 >= s(2.0), "{t:?}");
        // still no overlaps
        for w in t.windows(2) {
            assert!(w[0].1 <= w[1].0, "{t:?}");
        }
    }

    #[test]
    fn the_last_caption_stops_at_the_sequence_end_and_short_ones_get_a_second() {
        let b = [Block { start: s(10.0), end: s(10.2), chars: 3 }];
        let (t, _) = readable_times(&b, rate(), Some(s(10.6)));
        assert_eq!(t[0], (s(10.0) - (s(1.0) - s(0.6)), s(10.6)));
        let (t, still) = readable_times(&b, rate(), None);
        assert_eq!(t[0], (s(10.0), s(11.0)), "at least one second on screen");
        assert!(still.is_empty());
    }

    #[test]
    fn times_land_on_frames_and_hostile_values_do_not_panic() {
        let b = [Block { start: Tick(7), end: Tick(13), chars: usize::MAX }, Block { start: Tick(i64::MAX - 10), end: Tick(i64::MAX), chars: 50 }];
        let (t, _) = readable_times(&b, rate(), None);
        assert_eq!(rate().snap(t[0].1), t[0].1);
        assert_eq!(readable_times(&[], rate(), None), (vec![], vec![]));
    }
}
