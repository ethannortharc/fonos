//! Float-indicator geometry — the single source of truth for how big the pill
//! window is and where it sits.
//!
//! ## Why this module exists
//!
//! The pill's size used to be a three-way lockstep that nothing enforced:
//! `tauri.conf.json`'s 98×32 float window ↔ `float.html`'s `IDLE_W`/`PH` ↔ a
//! hardcoded `98.0`/`32.0` pair inside the placement math. Adding a second idle
//! shape would have made it a four-way one. The sizes now live here; Rust reads
//! them directly and the front end fetches them once via `float_geometry`, so a
//! shape's dimensions are written down exactly once.
//!
//! ## The window *is* the hit area
//!
//! The indicator has no surrounding dead zone: the window is sized to the
//! region that should accept clicks, and everything outside it is simply not
//! our window, so clicks land on whatever is behind. A 44×3 hairline therefore
//! lives in a 44×14 window — the extra 5.5pt above and below is deliberate
//! target padding (a 3pt-tall click target is not hittable), and it is also the
//! *entire* extent of what the indicator can steal a click from.
//!
//! This is why no cursor polling or `ignore_cursor_events` machinery appears
//! anywhere: per-shape hit regions fall out of window sizing for free.
//!
//! ## Everything here is LOGICAL points, never physical pixels
//!
//! This is load-bearing on a mixed-DPI desktop, not a stylistic choice. Tao
//! reports each monitor's origin as `PhysicalPosition::from_logical(bounds,
//! that_monitor.scale_factor())`, but converts `set_outer_position(Physical)`
//! using *the window's current* scale factor. The two disagree the moment two
//! displays have different scales, and the "physical" monitor rects are not
//! even self-consistent: a 2× built-in 1512pt wide spans physical 0..3024,
//! while a 1× external sitting immediately to its right at logical x=1512
//! reports physical x=1512 — the rects overlap, so a point can appear to be on
//! the wrong display.
//!
//! macOS's global coordinate space is logical to begin with (`CGDisplayBounds`
//! returns points); tao's per-monitor physical conversion is what introduces
//! the inconsistency. Dividing each monitor's reported bounds by its own scale
//! recovers the coherent space, and `set_outer_position(Logical)` passes
//! through tao's conversion untouched. So: monitor rects, the anchor, the
//! persisted position and every function below are points.
//!
//! A pleasant side effect — [`transplant_anchor`] needs no scale arithmetic at
//! all, because a point is a point on every display.
//!
//! ## Anchors are centers
//!
//! Placement is expressed as the **center of the visible mark**, not its
//! top-left corner, because the window changes size constantly (a 44×14
//! hairline grows into a 122×32 recording pill and back). Anchoring the center
//! makes the pill bloom symmetrically out of the mark instead of lurching, and
//! keeps a user-dragged position from drifting as the window resizes under it.

use fonos_core::config::{FLOAT_DOT, FLOAT_HAIRLINE, FLOAT_OFF, FLOAT_PILL};

/// A monitor's bounds in logical points: `(x, y, width, height)`.
pub type Rect = (f64, f64, f64, f64);
/// A point in the logical global coordinate space.
pub type Point = (f64, f64);
/// A window size in logical points.
pub type Size = (f64, f64);

// ── Idle window sizes ───────────────────────────────────────────────────────
//
// Each pair is the window size for that shape, which is also its hit area (see
// the module docs). The *visible* mark inside is smaller; that part lives in
// float.html's CSS, since only paint cares about it.

/// Hairline: a bare 44×3 rule, padded to a hittable 44×14 window.
pub const HAIRLINE: Size = (44.0, 14.0);
/// Dot: a 14×14 waveform glyph, padded to a 24×24 window.
pub const DOT: Size = (24.0, 24.0);
/// Pill: the full glyph + `FONOS` wordmark, unchanged from pre-0.8.7.
pub const PILL: Size = (98.0, 32.0);

/// Height of the working-state pill (recording / processing / result). Shared
/// by every shape: once there is something to report, the indicator always
/// expands to the full pill, because that is precisely when its pixels are the
/// ones you want to see. Mirrors `float.html`'s `PH`.
pub const WORKING_H: f64 = 32.0;

/// Gap between the monitor's bottom edge and the *bottom of the working pill* —
/// enough to clear the system's own bottom furniture plus a little air. Both
/// values are carried over verbatim from the per-platform placement code this
/// module replaced, so upgrading moves nothing.
#[cfg(target_os = "macos")]
pub const DOCK_CLEARANCE: f64 = 110.0;
/// See the macOS variant: approximate Linux taskbar height.
#[cfg(not(target_os = "macos"))]
pub const DOCK_CLEARANCE: f64 = 48.0;

/// The idle window size for `form`.
///
/// `FLOAT_OFF` has no window on screen, but still returns a size: callers that
/// compute geometry before consulting visibility (and the front end, which
/// asks for every shape's size up front) need a total function here.
pub fn idle_size(form: &str) -> Size {
    match fonos_core::config::normalize_float_indicator(form) {
        FLOAT_DOT => DOT,
        FLOAT_PILL => PILL,
        FLOAT_HAIRLINE => HAIRLINE,
        // Hidden; the value is never painted, but keep it the default shape's
        // so nothing downstream has to special-case a zero size.
        FLOAT_OFF => PILL,
        // Anything normalize_float_indicator rejected.
        _ => PILL,
    }
}

/// The size to keep an anchor inside a display against.
///
/// Normally the idle window, since that is what sits there. For `"off"` there
/// is no idle window, so the honest answer is the working pill — the only thing
/// that ever appears. Using `idle_size`'s stand-in there would be clamping
/// against a window that never exists.
pub fn clamp_size(form: &str) -> Size {
    if shows_when_idle(form) { idle_size(form) } else { (PILL.0, WORKING_H) }
}

/// Whether `form` shows a window while the indicator is IDLE.
///
/// Not "is it ever visible": `"off"` still surfaces for the working states.
/// Hiding the indicator during a dictation too would mean pressing a hotkey and
/// having text appear with no acknowledgement anywhere on screen — "off" is
/// about not occupying space while there is nothing to say, not about
/// withholding feedback while there is.
pub fn shows_when_idle(form: &str) -> bool {
    fonos_core::config::normalize_float_indicator(form) != FLOAT_OFF
}

/// Every shape's idle size, as `(name, w, h)` triples — the payload the front
/// end fetches so `float.html` never restates a number this module owns.
pub fn all_idle_sizes() -> [(&'static str, f64, f64); 4] {
    [
        (FLOAT_HAIRLINE, HAIRLINE.0, HAIRLINE.1),
        (FLOAT_DOT, DOT.0, DOT.1),
        (FLOAT_PILL, PILL.0, PILL.1),
        (FLOAT_OFF, PILL.0, PILL.1),
    ]
}

/// The default anchor for a monitor: horizontally centered, sitting the same
/// distance above the bottom edge that the pre-0.8.7 pill did.
///
/// The vertical term is derived from the working pill's height rather than the
/// current shape's, so every shape's mark lands on the identical point —
/// switching from pill to hairline moves nothing, and the pill still occupies
/// exactly the points it always has.
pub fn default_anchor((mx, my, mw, mh): Rect) -> Point {
    (mx + mw / 2.0, my + mh - DOCK_CLEARANCE - WORKING_H / 2.0)
}

/// Top-left position that centers a `size` window on `anchor`. The inverse of
/// [`center_of`].
pub fn top_left_for(anchor: Point, size: Size) -> Point {
    top_left_for_offset(anchor, size.0, size.1 / 2.0)
}

/// Top-left for a window whose anchor point sits `offset_y` below its top edge
/// (still horizontally centered).
///
/// The general form of [`top_left_for`], needed because the mark is not always
/// in the middle of the window: opening the workflow menu grows the window
/// upward, with the mark left sitting near its bottom edge. Anchoring on the
/// window's center there would shove the mark downward by half the menu's
/// height the instant it opened.
pub fn top_left_for_offset(anchor: Point, w: f64, offset_y: f64) -> Point {
    (anchor.0 - w / 2.0, anchor.1 - offset_y)
}

/// Center of a window given its top-left position and size. The inverse of
/// [`top_left_for`]; used to recover the anchor after the user drags the
/// window, since the drag is performed natively and reports no anchor.
pub fn center_of(top_left: Point, size: Size) -> Point {
    (top_left.0 + size.0 / 2.0, top_left.1 + size.1 / 2.0)
}

/// Pull `anchor` far enough inside `mon` that a `size` window centered on it is
/// fully on screen.
///
/// Applied whenever a monitor's bounds might no longer contain a remembered
/// position — a dragged-to-the-edge indicator after a resolution change, or one
/// inherited from a larger display. Clamping (rather than re-centering) keeps
/// the user's chosen corner: they land at the nearest legal point to it, not
/// back in the middle of the screen.
pub fn clamp_anchor(anchor: Point, size: Size, mon: Rect) -> Point {
    let tl = clamp_top_left(top_left_for(anchor, size), size, mon);
    center_of(tl, size)
}

/// Pull a window's top-left corner inside `mon` so the whole window is on
/// screen.
///
/// Distinct from [`clamp_anchor`] because the *anchor* must stay where the user
/// dropped it while transient, larger geometry still has to fit: a hairline
/// legally parked 22pt from the left edge would put the 122pt recording pill at
/// x = −39 and the 168pt workflow menu at x = −62. Clamping the rect at resize
/// time nudges those bigger states back on screen without moving the anchor the
/// small idle shape returns to.
///
/// A window larger than the monitor is pinned to its top-left, which is the
/// only choice that keeps the origin visible when nothing else can be.
pub fn clamp_top_left((x, y): Point, (w, h): Size, (mx, my, mw, mh): Rect) -> Point {
    (
        if w >= mw { mx } else { x.clamp(mx, mx + mw - w) },
        if h >= mh { my } else { y.clamp(my, my + mh - h) },
    )
}

/// Index of the first rect containing `(cx, cy)`. Left/top inclusive,
/// right/bottom exclusive.
pub fn rect_index_containing(rects: &[Rect], cx: f64, cy: f64) -> Option<usize> {
    rects
        .iter()
        .position(|(x, y, w, h)| cx >= *x && cx < *x + *w && cy >= *y && cy < *y + *h)
}

/// Carry a deliberately-chosen position across to another display: the point on
/// the new display sitting the same distance from *its* default anchor as
/// `anchor` sits from `old_default` on the old one.
///
/// Used when the indicator follows the cursor onto another monitor — someone who
/// parked it in a corner wants a corner on the new screen too, not the default
/// spot back in the middle.
///
/// Callers must always pass the user's *home* anchor here, never the result of
/// a previous transplant. Chaining is lossy: the clamp applied on a narrow
/// display would permanently shrink the offset, so hopping to a small screen
/// and back would not return the indicator to where it started.
pub fn transplant_anchor(anchor: Point, old_default: Point, new_default: Point) -> Point {
    (
        new_default.0 + (anchor.0 - old_default.0),
        new_default.1 + (anchor.1 - old_default.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare points at sub-point precision — the math is exact for the sizes
    /// in play, but spelling the tolerance out beats brittle float equality.
    fn close(a: Point, b: Point) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn idle_sizes_map_to_their_shapes() {
        assert_eq!(idle_size(FLOAT_HAIRLINE), HAIRLINE);
        assert_eq!(idle_size(FLOAT_DOT), DOT);
        assert_eq!(idle_size(FLOAT_PILL), PILL);
    }

    #[test]
    fn unknown_shape_falls_back_to_the_default() {
        assert_eq!(idle_size("some-future-shape"), PILL);
        assert_eq!(idle_size(""), PILL);
    }

    #[test]
    fn clamp_size_falls_back_to_the_working_pill_when_nothing_shows_idle() {
        assert_eq!(clamp_size(FLOAT_HAIRLINE), HAIRLINE);
        assert_eq!(clamp_size(FLOAT_PILL), PILL);
        // "off" never draws an idle window, so its idle size is a stand-in;
        // the working pill is the only geometry that actually appears.
        assert_eq!(clamp_size(FLOAT_OFF), (PILL.0, WORKING_H));
    }

    #[test]
    fn only_off_is_hidden_while_idle() {
        assert!(shows_when_idle(FLOAT_HAIRLINE));
        assert!(shows_when_idle(FLOAT_DOT));
        assert!(shows_when_idle(FLOAT_PILL));
        assert!(!shows_when_idle(FLOAT_OFF));
        // Junk normalizes to the default shape, which is visible — never blank.
        assert!(shows_when_idle("nonsense"));
    }

    #[test]
    fn hairline_hit_area_is_a_quarter_of_the_old_pill() {
        // The whole point of the feature: the window is the hit area, so this
        // ratio is literally how much less screen the indicator can steal a
        // click from. Guards against someone "tidying" the padding away.
        let old = PILL.0 * PILL.1;
        let new = HAIRLINE.0 * HAIRLINE.1;
        assert!(new / old < 0.25, "hairline hit area regressed: {new} vs {old}");
    }

    #[test]
    fn hairline_window_is_tall_enough_to_click() {
        // 3pt is the visible rule; the window must stay meaningfully taller or
        // the indicator becomes unhittable (and undraggable).
        assert!(HAIRLINE.1 >= 12.0, "hairline target too short to hit");
    }

    #[test]
    fn default_anchor_matches_the_legacy_pill_placement() {
        // Pre-0.8.7 placed the pill's TOP-LEFT at y = mon_h - 32 - CLEARANCE,
        // so its center sat 16 lower. Every shape must land on that same point,
        // or upgrading would visibly shift the indicator.
        let (cx, cy) = default_anchor((0.0, 0.0, 1440.0, 900.0));
        assert_eq!(cx, 720.0);
        assert_eq!(cy, 900.0 - DOCK_CLEARANCE - 16.0);
        // Pin the per-platform constants themselves — the two numbers carried
        // over from the code this module replaced.
        #[cfg(target_os = "macos")]
        assert_eq!(cy, 900.0 - 110.0 - 16.0, "macOS Dock clearance changed");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(cy, 900.0 - 48.0 - 16.0, "Linux taskbar clearance changed");
    }

    #[test]
    fn default_anchor_respects_a_monitor_origin_offset() {
        // Second display to the right: the anchor must be centered on *it*.
        let (cx, cy) = default_anchor((1512.0, 0.0, 1920.0, 1080.0));
        assert_eq!(cx, 1512.0 + 960.0);
        assert_eq!(cy, 1080.0 - DOCK_CLEARANCE - 16.0);
    }

    #[test]
    fn top_left_and_center_round_trip() {
        for size in [HAIRLINE, DOT, PILL, (122.0, 32.0), (117.0, 32.0)] {
            let anchor = (700.5, 800.0);
            assert!(close(center_of(top_left_for(anchor, size), size), anchor), "{size:?}");
        }
    }

    #[test]
    fn odd_widths_round_trip_without_drift() {
        // Error pills are sized from their text and are routinely odd. The
        // anchor is the stored source of truth, so repeatedly resizing must
        // never walk the window sideways — the exact drift the old
        // "re-derive the center from the current rect" approach risked.
        let anchor = (701.0, 803.0);
        let mut tl = top_left_for(anchor, HAIRLINE);
        for w in [117.0, 44.0, 133.0, 44.0, 99.0, 44.0] {
            tl = top_left_for(anchor, (w, 14.0));
        }
        assert!(close(center_of(tl, HAIRLINE), anchor));
        assert!(close(tl, top_left_for(anchor, HAIRLINE)));
    }

    // ── Clamping ────────────────────────────────────────────────────────────

    const SCREEN: Rect = (0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn clamp_keeps_an_in_bounds_anchor_untouched() {
        assert!(close(clamp_anchor((700.0, 800.0), HAIRLINE, SCREEN), (700.0, 800.0)));
    }

    #[test]
    fn clamp_pulls_an_off_screen_anchor_just_inside() {
        let a = clamp_anchor((5000.0, 5000.0), HAIRLINE, SCREEN);
        assert!(close(a, (1440.0 - 22.0, 900.0 - 7.0)));
        // And the window really is fully on screen at that anchor.
        let (x, y) = top_left_for(a, HAIRLINE);
        assert!(x >= 0.0 && x + HAIRLINE.0 <= 1440.0);
        assert!(y >= 0.0 && y + HAIRLINE.1 <= 900.0);
    }

    #[test]
    fn clamp_handles_the_negative_origin_of_a_left_hand_display() {
        let a = clamp_anchor((-9999.0, -9999.0), HAIRLINE, (-1920.0, 0.0, 1920.0, 1080.0));
        assert!(close(a, (-1920.0 + 22.0, 7.0)));
    }

    #[test]
    fn a_window_larger_than_its_monitor_pins_to_the_origin() {
        assert!(close(clamp_top_left((-50.0, -50.0), (400.0, 400.0), (0.0, 0.0, 200.0, 200.0)), (0.0, 0.0)));
    }

    #[test]
    fn a_bigger_state_is_reclamped_even_though_its_anchor_is_legal() {
        // The regression this exists for: a hairline dropped hard against the
        // left edge has a perfectly legal anchor at x=22, but the 122pt
        // recording pill centered there starts at x=−39, and the 168pt menu at
        // x=−62. Both must be nudged back on screen.
        let anchor = clamp_anchor((0.0, 800.0), HAIRLINE, SCREEN);
        assert!(close(anchor, (22.0, 800.0)), "hairline anchor: {anchor:?}");
        for size in [(122.0, 32.0), (168.0, 150.0), PILL] {
            let raw = top_left_for(anchor, size);
            assert!(raw.0 < 0.0, "test is vacuous for {size:?}");
            let (x, _) = clamp_top_left(raw, size, SCREEN);
            assert_eq!(x, 0.0, "{size:?} left edge not pulled on screen");
        }
    }

    // ── Transplanting across displays ───────────────────────────────────────

    #[test]
    fn transplant_preserves_the_offset() {
        let old_default = (720.0, 774.0);
        let anchor = (1100.0, 500.0); // dragged +380 right, −274 up
        let moved = transplant_anchor(anchor, old_default, (2400.0, 964.0));
        assert!(close(moved, (2400.0 + 380.0, 964.0 - 274.0)));
    }

    #[test]
    fn transplant_of_an_unmoved_anchor_is_the_new_default() {
        let d = (720.0, 774.0);
        assert!(close(transplant_anchor(d, d, (2400.0, 964.0)), (2400.0, 964.0)));
    }

    #[test]
    fn transplanting_from_home_survives_a_round_trip_via_a_smaller_display() {
        // The lossiness this rule exists to prevent: park the indicator near
        // the right edge of a wide display, visit a narrow one (where the
        // offset gets clamped away), and come back. Because every transplant
        // starts from the HOME anchor rather than the last computed one, the
        // return trip lands exactly where it started.
        let wide: Rect = (0.0, 0.0, 3840.0, 2160.0);
        let narrow: Rect = (3840.0, 0.0, 1440.0, 900.0);
        let home = (3818.0, 2000.0);

        let on_narrow = clamp_anchor(
            transplant_anchor(home, default_anchor(wide), default_anchor(narrow)),
            HAIRLINE,
            narrow,
        );
        assert!(on_narrow.0 < 3840.0 + 1440.0, "should have been clamped inside");

        let back = clamp_anchor(
            transplant_anchor(home, default_anchor(wide), default_anchor(wide)),
            HAIRLINE,
            wide,
        );
        assert!(close(back, home), "round trip lost the position: {back:?}");
    }

    // ── Real multi-display scenarios ────────────────────────────────────────
    //
    // Ported from the `display_tests` module that lived alongside the placement
    // code this module replaced. Kept as concrete hardware configurations
    // rather than folded into the abstract tests above, because the
    // stranded-anchor case is a bug that was actually reported.

    /// Built-in laptop display at the origin, 1512×982 @ 2x.
    const BUILTIN: Rect = (0.0, 0.0, 1512.0, 982.0);
    /// External 4K to the right of the built-in, 3840×2160 @ 1x.
    const EXTERNAL: Rect = (1512.0, 0.0, 3840.0, 2160.0);

    #[test]
    fn mixed_dpi_monitor_rects_do_not_overlap_in_logical_space() {
        // The reason this module is logical-only. Tao reports the built-in's
        // width as 3024 "physical" and the external's origin as 1512
        // "physical" — overlapping rects, in which a point at x=2000 resolves
        // to the WRONG display. In points the two are disjoint and adjacent.
        assert_eq!(BUILTIN.0 + BUILTIN.2, EXTERNAL.0, "displays must be adjacent");
        let rects = [BUILTIN, EXTERNAL];
        assert_eq!(rect_index_containing(&rects, 2000.0, 100.0), Some(1));
        assert_eq!(rect_index_containing(&rects, 1000.0, 100.0), Some(0));
    }

    #[test]
    fn anchor_on_external_selects_external() {
        assert_eq!(rect_index_containing(&[BUILTIN, EXTERNAL], 1512.0 + 1920.0, 1080.0), Some(1));
    }

    #[test]
    fn stranded_anchor_on_a_dead_display_selects_none() {
        // External unplugged: only the built-in survives, but the anchor is
        // still parked at coordinates that were inside the (now gone) 4K.
        // Selection must return None so the caller falls back to the primary
        // monitor rather than leaving the indicator off-screen.
        assert_eq!(rect_index_containing(&[BUILTIN], 1512.0 + 1920.0, 1080.0), None);
    }

    #[test]
    fn empty_monitor_set_selects_none() {
        assert_eq!(rect_index_containing(&[], 100.0, 100.0), None);
    }

    #[test]
    fn rect_index_containing_uses_half_open_bounds() {
        let rects = [BUILTIN, EXTERNAL];
        assert_eq!(rect_index_containing(&rects, 0.0, 0.0), Some(0), "top-left inclusive");
        assert_eq!(rect_index_containing(&rects, 1512.0, 10.0), Some(1), "left edge inclusive");
        assert_eq!(rect_index_containing(&rects, 5352.0, 10.0), None, "right edge exclusive");
        assert_eq!(rect_index_containing(&rects, 10.0, 982.0), None, "bottom edge exclusive");
    }
}
