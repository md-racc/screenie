//! Select's behaviour, driven like a user: `--keys` scripts, and raw
//! events where timing matters. Ported from the prototype's
//! tests/test_modes.py, tests/test_audit.py and tests/test_recall.py (at
//! the `python-prototype` tag).

mod common;

use common::*;
use screenie_core::geometry::{PixelPoint, PixelRect};
use screenie_core::input::{DELAY, RAMP};
use screenie_core::keys::Mods;
use screenie_core::session::{Button, FinishedAction, Outcome, RawEvent, Session};

fn done(s: &Session) -> Option<&FinishedAction> {
    match s.outcome() {
        Some(Outcome::Done(a)) => Some(a),
        _ => None,
    }
}

// --- the box ------------------------------------------------------------------

#[test]
fn the_status_shows_the_mode_size_and_cursor_pixel() {
    assert_eq!(session().status(), "SELECT  560×90  (659,259)");
}

#[test]
fn the_tag_tracks_the_cursor_corner() {
    let mut s = session();
    let at = |s: &Session| s.cursor_pixel().unwrap();
    s.keys("o");
    assert_eq!(at(&s), PixelPoint::new(100, 170)); // top-left
    s.keys("O");
    assert_eq!(at(&s), PixelPoint::new(659, 170)); // top-right
    s.keys("space"); // Space is o: the opposite corner
    assert_eq!(at(&s), PixelPoint::new(100, 259)); // bottom-left
    s.keys("space");
    assert_eq!(at(&s), PixelPoint::new(659, 170));
    s.keys("<S-space>"); // Shift+Space is O: the corner sideways
    assert_eq!(at(&s), PixelPoint::new(100, 170));
    assert!(s.status().contains("(100,170)"));
}

#[test]
fn the_modifier_grammar() {
    let mut s = session();
    let (x, y, w, h) = (START.x, START.y, START.w, START.h);
    s.keys("<A-h>"); // Alt: halve
    assert_eq!(s.rect(), r(x, y, w / 2.0, h));
    s.keys("<A-S-l>"); // Alt+Shift: double
    assert_eq!(s.rect(), r(x, y, w, h));
    s.keys("<S-l>"); // Shift: move the box
    assert_eq!(s.rect(), r(x + 5.0, y, w, h));
    s.keys("L"); // the same, as GTK spells it
    assert_eq!(s.rect(), r(x + 10.0, y, w, h));
    s.keys("l"); // plain: the cursor corner moves
    assert_eq!(s.rect(), r(x + 10.0, y, w + 5.0, h));
    s.keys("Left"); // the arrows are hjkl
    assert_eq!(s.rect(), r(x + 10.0, y, w, h));
}

#[test]
fn ctrl_jumps_to_the_neighbouring_window_and_ctrl_shift_slides() {
    let mut s = session_at(r(2000.0, 100.0, 200.0, 100.0), vec![]);
    s.keys("<C-l>");
    assert_eq!(s.rect(), WINDOWS[1]);
    s.keys("<C-h>");
    assert_eq!(s.rect(), WINDOWS[0]);
    let mut s = session_at(r(100.0, 100.0, 300.0, 100.0), vec![]);
    s.keys("<C-S-j>"); // bottom 200 -> 270, a quarter line
    assert_eq!(s.rect(), r(100.0, 170.0, 300.0, 100.0));
}

#[test]
fn a_meaningless_modifier_does_nothing_at_all() {
    let mut s = session();
    s.keys("<C-o> <C-A-l> <A-Return> <C-c> <C-q>");
    assert_eq!(s.rect(), START);
    assert!(!s.selection().is_centred());
    assert_eq!(s.cursor_pixel().unwrap(), PixelPoint::new(659, 259));
    assert_eq!(s.outcome(), None);
}

#[test]
fn edge_and_far_jumps() {
    let mut s = session_at(r(2000.0, 100.0, 200.0, 100.0), vec![]);
    s.keys("w");
    assert_eq!(s.rect().right(), 2878.0);
    s.keys("w"); // across the 5px gap: skipped
    assert_eq!(s.rect().right(), 3837.0);
    s.keys("G");
    assert_eq!(s.rect().bottom(), 1080.0);
    s.keys("0"); // the cursor crosses the anchor to the far left
    assert_eq!(s.rect(), r(0.0, 100.0, 2000.0, 980.0));
    s.keys("dollar");
    assert_eq!(s.rect(), r(2000.0, 100.0, 1840.0, 980.0));
    s.keys("g");
    assert_eq!(s.rect(), r(2000.0, 0.0, 1840.0, 100.0));
    s.keys("braceright"); // } : the next edge down is the windows' 1077
    assert_eq!(s.rect().bottom(), 1077.0);
}

#[test]
fn centre_mode_resizes_around_the_centre() {
    let mut s = session_at(r(2000.0, 100.0, 200.0, 100.0), vec![]);
    s.keys("c h");
    assert_eq!(s.rect(), r(1995.0, 100.0, 210.0, 100.0));
    s.keys("j");
    assert_eq!(s.rect(), r(1995.0, 105.0, 210.0, 90.0));
    assert!(s.status().ends_with("  centre"));
    assert_eq!(s.cursor_pixel().unwrap(), PixelPoint::new(2100, 150)); // the centre
}

// --- scope ----------------------------------------------------------------------

#[test]
fn plus_minus_and_the_wheel_change_the_scope() {
    let mut s = session();
    s.keys("plus");
    assert_eq!(s.rect(), WINDOWS[2]); // the window the box is in
    s.keys("wheel:1");
    assert_eq!(s.rect(), MONS[0]); // its monitor
    s.keys("wheel:1");
    assert_eq!(s.rect(), LAYOUT); // everything
    s.keys("minus"); // down from everything: the monitor at its centre
    assert_eq!(s.rect(), MONS[1]);
    s.keys("wheel:-2"); // its nearest window, and - stops there
    assert_eq!(s.rect(), WINDOWS[0]);
    s.keys("equal KP_Add"); // + by other names
    assert_eq!(s.rect(), LAYOUT);
}

#[test]
fn scrolling_down_a_drawn_box_leaves_it() {
    let mut s = session();
    s.keys("drag:200,300:400,500");
    s.keys("minus");
    assert_eq!(s.rect(), r(200.0, 300.0, 200.0, 200.0));
}

// --- the mouse --------------------------------------------------------------------

#[test]
fn a_drag_draws_a_region() {
    let mut s = session();
    s.keys("drag:150,180:300,240");
    assert_eq!(s.rect(), r(150.0, 180.0, 150.0, 60.0));
    assert_eq!(s.outcome(), None); // a drag never finishes anything
    // Up and left, from off the box: the cursor is top-left.
    s.keys("drag:300,300:100,100");
    assert_eq!(s.rect(), r(100.0, 100.0, 200.0, 200.0));
    assert_eq!(s.cursor_pixel().unwrap(), PixelPoint::new(100, 100));
}

#[test]
fn a_drag_is_fractional() {
    let mut s = session();
    s.keys("drag:150.25,180.5:300.75,240");
    assert_eq!(s.rect(), r(150.25, 180.5, 150.5, 59.5));
}

#[test]
fn a_click_snaps_to_the_window_under_it_or_the_monitor() {
    let mut s = session();
    s.keys("click:2000,500");
    assert_eq!(s.rect(), WINDOWS[0]);
    s.keys("click:1000,500"); // no window between 958 and 1923
    assert_eq!(s.rect(), MONS[0]);
}

#[test]
fn a_press_that_barely_moves_is_still_a_click() {
    let mut s = session();
    s.button(Button::Left, p(2000.0, 500.0), true);
    s.motion(p(2002.0, 501.0)); // 3px: not a drag yet
    s.button(Button::Left, p(2002.0, 501.0), false);
    assert_eq!(s.rect(), WINDOWS[0]);
}

#[test]
fn motion_without_a_button_does_nothing() {
    let mut s = session();
    s.keys("drag:200,300:400,400");
    let before = s.rect();
    s.motion(p(900.0, 700.0));
    assert_eq!(s.rect(), before);
}

#[test]
fn right_click_is_enter_and_middle_click_is_esc() {
    let mut s = session();
    s.keys("rclick");
    assert_eq!(done(&s), Some(&FinishedAction::Image { rect: START }));
    let mut s = session();
    s.button(Button::Middle, p(500.0, 500.0), true);
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
}

// --- finishing -------------------------------------------------------------------

#[test]
fn the_finishing_keys() {
    let cases = [
        ("Return", FinishedAction::Image { rect: START }),
        ("KP_Enter", FinishedAction::Image { rect: START }),
        ("y", FinishedAction::Ocr { rect: START }),
        ("P", FinishedAction::Pin { rect: START }),
        (
            "p",
            FinishedAction::Colour {
                pixel: PixelPoint::new(659, 259),
            },
        ),
    ];
    for (key, want) in cases {
        let mut s = session();
        s.keys(key);
        assert_eq!(done(&s), Some(&want), "{key}");
    }
}

struct Reads(Option<&'static str>);

impl screenie_core::session::CodeReader for Reads {
    fn read(
        &mut self,
        _: &Session,
        _: screenie_core::geometry::LogicalRect,
    ) -> Result<Option<String>, String> {
        Ok(self.0.map(String::from))
    }
}

#[test]
fn q_reads_a_code_or_says_none_was_found_and_stays() {
    let mut s = session();
    s.set_code_reader(Box::new(Reads(None)));
    s.keys("Q");
    assert_eq!(
        (s.outcome(), s.note()),
        (None, Some("no QR / barcode found"))
    );
    let mut s = session();
    s.set_code_reader(Box::new(Reads(Some("https://example.org"))));
    s.keys("Q");
    assert_eq!(
        done(&s),
        Some(&FinishedAction::Code {
            rect: START,
            text: "https://example.org".into()
        })
    );
}

#[test]
fn esc_and_q_cancel() {
    for key in ["Escape", "q"] {
        let mut s = session();
        s.keys(key);
        assert_eq!(s.outcome(), Some(&Outcome::Cancelled), "{key}");
    }
}

#[test]
fn nothing_happens_after_the_session_finishes() {
    let mut s = session();
    s.keys("Return l Escape");
    assert_eq!(done(&s), Some(&FinishedAction::Image { rect: START }));
    assert_eq!(s.rect(), START);
}

#[test]
fn a_sub_pixel_box_does_not_capture() {
    let mut s = session_at(r(100.5, 170.0, 0.6, 90.0), vec![]);
    for key in ["Return", "y", "P", "Q"] {
        s.keys(key);
        assert_eq!(s.outcome(), None, "{key}");
        assert_eq!(s.note(), Some("empty box"));
    }
    s.keys("l"); // the next key clears the note
    assert_eq!(s.note(), None);
}

// --- keys and repeat -----------------------------------------------------------------

#[test]
fn autorepeat_does_not_flicker_toggles() {
    let mut s = session();
    s.press(7, "c", Mods::NONE, 0.0);
    s.press(7, "c", Mods::NONE, 0.5); // the platform's autorepeat: no release
    s.press(7, "c", Mods::NONE, 0.6);
    assert!(s.selection().is_centred());
    s.release(7, "c", Mods::NONE, 0.7);
    s.keys("c");
    assert!(!s.selection().is_centred());
}

#[test]
fn a_held_direction_repeats_with_acceleration_and_is_one_undo() {
    let mut s = session();
    s.press(1, "l", Mods::NONE, 0.0);
    assert_eq!(s.rect().w, 565.0); // the tap: exactly one step
    let mut t = 0.0;
    while t < DELAY + RAMP {
        t += 1.0 / 60.0;
        s.tick(t.min(DELAY + RAMP));
    }
    assert_eq!(s.rect().w, 565.0 + 96.0 * 5.0); // the ramp, 20 to 300 steps/s
    s.release(1, "l", Mods::NONE, t);
    assert!(!s.tick(t + 1.0));
    s.keys("u");
    assert_eq!(s.rect(), START);
}

#[test]
fn held_directions_combine_and_shift_moves_the_box() {
    let mut s = session_at(r(1000.0, 500.0, 100.0, 100.0), vec![]);
    s.press(50, "Shift_L", Mods::SHIFT, 0.0);
    s.press(1, "k", Mods::SHIFT, 0.0);
    s.press(2, "l", Mods::SHIFT, 0.0);
    s.press(3, "h", Mods::SHIFT, 0.0); // cancels l
    s.tick(DELAY);
    s.tick(DELAY + 0.1); // 2 steps at the start of the ramp
    let r1 = s.rect();
    assert_eq!((r1.w, r1.h), (100.0, 100.0)); // moved, not resized
    assert_eq!(r1.x, 1000.0); // h and l cancel
    assert!(r1.y < 500.0 - 5.0); // up, past the tap
}

#[test]
fn shift_from_a_separate_modifiers_event_moves_the_held_box() {
    // Wayland: the Shift key's press, then a modifiers event saying so.
    let mut s = session_at(r(1000.0, 500.0, 100.0, 100.0), vec![]);
    s.press(50, "Shift_L", Mods::NONE, 0.0);
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    s.press(1, "L", Mods::SHIFT, 0.0);
    s.tick(DELAY + 0.1);
    assert_eq!(s.rect().w, 100.0);
    assert!(s.rect().x > 1005.0);
}

#[test]
fn a_digit_typed_with_shift_is_the_digit() {
    // AZERTY's digit row needs Shift.
    let mut s = session();
    s.press(50, "Shift_L", Mods::SHIFT, 0.0);
    s.press(2, "1", Mods::SHIFT, 0.0);
    assert_eq!(s.note(), Some("no layers yet"));
    s.press(11, "0", Mods::SHIFT, 0.0); // far left
    assert_eq!(s.rect().x, 0.0);
}

const SUPER: Mods = Mods {
    logo: true,
    ..Mods::NONE
};

#[test]
fn a_key_with_super_held_does_nothing() {
    let mut s = session();
    s.press(60, "Super_L", SUPER, 0.0);
    s.press(1, "l", SUPER, 0.0);
    assert!(!s.tick(1.0), "nor does it repeat");
    s.press(2, "Return", SUPER, 1.0);
    s.press(3, "q", SUPER, 1.0);
    assert_eq!((s.rect(), s.outcome()), (START, None));
}

#[test]
fn super_types_nothing_in_text() {
    use screenie_core::model::OpData;
    let mut s = session();
    s.keys("a i");
    s.press(60, "Super_L", SUPER, 0.0);
    s.press(1, "z", SUPER, 0.0);
    s.release(60, "Super_L", Mods::NONE, 0.0);
    s.keys("x Return");
    let ops = &s.open_layer().unwrap().ops;
    assert!(matches!(&ops[0].data, OpData::Text(t) if t.text == "x"));
}

#[test]
fn ctrl_and_alt_never_repeat() {
    let mut s = session_at(r(1000.0, 500.0, 100.0, 100.0), vec![]);
    s.press(50, "Alt_L", Mods::ALT, 0.0);
    s.press(1, "h", Mods::ALT, 0.0); // halve once
    assert!(!s.tick(1.0));
    assert_eq!(s.rect(), r(1000.0, 500.0, 50.0, 100.0));
}

/// Ticks at 60 fps from `from` to `to`; returns `to`.
fn tick_to(s: &mut Session, from: f64, to: f64) -> f64 {
    let mut t = from;
    while t < to {
        t = (t + 1.0 / 60.0).min(to);
        s.tick(t);
    }
    to
}

#[test]
fn a_modifier_tapped_mid_hold_resumes_without_a_burst() {
    let mut s = session_at(r(1000.0, 500.0, 100.0, 100.0), vec![]);
    s.press(1, "l", Mods::NONE, 0.0);
    let t = tick_to(&mut s, 0.0, 1.0);
    let w = s.rect().w;
    s.press(50, "Control_L", Mods::CTRL, t);
    s.tick(t + 1.0 / 60.0);
    s.release(50, "Control_L", Mods::NONE, t + 1.0 / 60.0);
    s.tick(t + 2.0 / 60.0);
    let grew = s.rect().w - w;
    assert!(grew <= 30.0, "grew {grew}");
}

#[test]
fn releasing_ctrl_first_after_a_held_jump_resumes_without_a_burst() {
    let mut s = session_at(r(1000.0, 500.0, 100.0, 100.0), vec![]);
    s.press(50, "Control_L", Mods::CTRL, 0.0);
    s.press(1, "l", Mods::CTRL, 0.0);
    let jumped = s.rect();
    let t = tick_to(&mut s, 0.0, 1.0);
    assert_eq!(s.rect(), jumped, "a jump doesn't repeat");
    s.release(50, "Control_L", Mods::NONE, t);
    s.tick(t + 1.0 / 60.0);
    let grew = s.rect().w - jumped.w;
    assert!(grew <= 30.0, "grew {grew}");
}

#[test]
fn question_mark_toggles_the_legend() {
    let mut s = session();
    assert!(!s.show_help());
    s.keys("question");
    assert!(s.show_help());
    assert!(s.legend()[0].starts_with("o/Space O/S-Space corner"));
    s.keys("?");
    assert!(!s.show_help());
}

// --- undo -------------------------------------------------------------------------

#[test]
fn undo_walks_back_through_selection_changes() {
    let mut s = session();
    s.keys("l plus drag:200,300:400,500 click:2000,500");
    for want in [
        r(200.0, 300.0, 200.0, 200.0),
        WINDOWS[2],
        r(100.0, 170.0, 565.0, 90.0),
        START,
    ] {
        s.keys("u");
        assert_eq!(s.rect(), want);
    }
    s.keys("u"); // nothing left: nothing happens
    assert_eq!(s.rect(), START);
}

#[test]
fn changes_that_change_nothing_are_not_undo_steps() {
    let mut s = session_at(r(0.0, 0.0, 100.0, 100.0), vec![]);
    s.keys("l <S-h> <S-h> minus"); // already at the left edge; - has no scope here
    s.keys("u");
    assert_eq!(s.rect(), r(0.0, 0.0, 100.0, 100.0));
}

// --- recall -----------------------------------------------------------------------

#[test]
fn dot_with_nothing_remembered_says_so() {
    let mut s = session();
    s.keys("period");
    assert_eq!(s.rect(), START);
    assert_eq!(s.note(), Some("no last selection"));
    assert!(s.status().ends_with("  (no last selection)"));
}

#[test]
fn dot_loads_the_last_box_then_older_ones_and_u_undoes() {
    let newest = r(2000.0, 50.0, 400.0, 300.0);
    let older = r(500.0, 500.0, 200.0, 100.0);
    let mut s = session_at(START, vec![newest, older]);
    s.keys("period");
    assert_eq!((s.rect(), s.note()), (newest, Some("last 1/2")));
    s.keys("period"); // again: the one before
    assert_eq!((s.rect(), s.note()), (older, Some("last 2/2")));
    s.keys("period"); // and round
    assert_eq!(s.rect(), newest);
    s.keys("l period"); // the box changed: . starts from the newest again
    assert_eq!(s.rect(), newest);
    s.keys("u u u u u"); // each . and the step were one change
    assert_eq!(s.rect(), START);
}

#[test]
fn dot_with_one_box_says_last() {
    let mut s = session_at(START, vec![r(10.0, 10.0, 50.0, 50.0)]);
    s.keys("c period");
    assert_eq!(s.note(), Some("last"));
    assert!(!s.selection().is_centred()); // . leaves centre mode
}

// --- the box by its edges (docs/mouse.md) --------------------------------------------

fn boxed() -> Session {
    session_at(r(100.0, 100.0, 400.0, 300.0), vec![])
}

#[test]
fn a_drag_from_an_edge_moves_only_that_edge() {
    let mut s = boxed();
    s.keys("drag:503,250:603,280"); // 3px outside the right edge
    assert_eq!(s.rect(), r(100.0, 100.0, 500.0, 300.0));
    let mut s = boxed();
    s.keys("drag:300,104:300,50"); // just inside the top
    assert_eq!(s.rect(), r(100.0, 46.0, 400.0, 354.0));
}

#[test]
fn a_drag_from_a_corner_moves_it_and_the_opposite_stays() {
    let mut s = boxed();
    s.keys("drag:500,400:550,450");
    assert_eq!(s.rect(), r(100.0, 100.0, 450.0, 350.0));
    let mut s = boxed();
    s.keys("drag:100,100:150,120");
    assert_eq!(s.rect(), r(150.0, 120.0, 350.0, 280.0));
}

#[test]
fn an_edge_crosses_the_opposite_one() {
    let mut s = boxed();
    s.keys("drag:100,250:600,250");
    assert_eq!(s.rect(), r(500.0, 100.0, 100.0, 300.0));
}

#[test]
fn alt_drag_inside_moves_the_box() {
    let mut s = boxed();
    s.handle(RawEvent::Modifiers { mods: Mods::ALT }, 0.0);
    s.keys("drag:200,200:250,230");
    s.handle(RawEvent::Modifiers { mods: Mods::NONE }, 0.0);
    assert_eq!(s.rect(), r(150.0, 130.0, 400.0, 300.0));
}

#[test]
fn centre_mode_makes_an_edge_drag_resize_about_the_centre() {
    let mut s = boxed();
    s.keys("c drag:500,250:520,250");
    assert_eq!(s.rect(), r(80.0, 100.0, 440.0, 300.0));
}

#[test]
fn an_edge_drag_is_one_undo_step() {
    let mut s = boxed();
    s.keys("drag:500,250:700,250");
    assert_eq!(s.rect(), r(100.0, 100.0, 600.0, 300.0));
    s.keys("u");
    assert_eq!(s.rect(), r(100.0, 100.0, 400.0, 300.0));
}

#[test]
fn away_from_the_edges_a_drag_still_draws_and_a_click_still_snaps() {
    let mut s = boxed();
    s.keys("drag:200,200:300,260"); // inside, no Alt: a new box
    assert_eq!(s.rect(), r(200.0, 200.0, 100.0, 60.0));
    let mut s = boxed();
    let mut t = boxed();
    s.keys("click:500,250"); // on an edge, but a click: the mode's
    t.keys("click:700,250");
    assert_eq!(s.rect(), t.rect());
}

// --- clickable palettes (docs/mouse.md) ----------------------------------------------

#[test]
fn a_palette_click_does_what_its_key_does_in_every_mode() {
    // Select, and the modes it opens that have palettes.
    for mode in ["", "x", "a", "a r", "f", "m"] {
        let mut probe = session();
        probe.keys(mode);
        let items = probe.palette_items();
        assert!(!items.is_empty(), "{mode:?} has a palette");
        for (text, intent) in items.into_iter().flatten() {
            let Some(intent) = intent else { continue };
            let key = match text.split(' ').next().unwrap() {
                "Enter" => "Return",
                k => k,
            };
            let (mut clicked, mut pressed) = (session(), session());
            clicked.keys(mode);
            pressed.keys(mode);
            clicked.act(intent);
            pressed.keys(key);
            assert_eq!(
                (clicked.mode_name(), clicked.status(), clicked.outcome()),
                (pressed.mode_name(), pressed.status(), pressed.outcome()),
                "{mode:?}: {text}"
            );
        }
    }
}

// --- Ctrl+Alt+hjkl: the cursor corner to edges in the picture ---------------------

/// What a finder was asked: from where, within what.
type Asked = std::rc::Rc<std::cell::RefCell<Vec<(PixelPoint, PixelRect)>>>;

/// Edges 50px from wherever it's asked, any way but up; what it was asked.
struct Every50(Asked);

impl screenie_core::measure::EdgeFinder for Every50 {
    fn edge(
        &mut self,
        _: &Session,
        from: PixelPoint,
        d: screenie_core::geometry::Dir,
        bounds: PixelRect,
    ) -> Option<PixelPoint> {
        self.0.borrow_mut().push((from, bounds));
        let (dx, dy) = d.vec();
        (dy >= 0.0).then(|| PixelPoint::new(from.x + 50 * dx as i32, from.y + 50 * dy as i32))
    }
}

fn with_edges() -> (Session, Asked) {
    let asked = std::rc::Rc::default();
    let mut s = boxed();
    s.set_edge_finder(Box::new(Every50(std::rc::Rc::clone(&asked))));
    (s, asked)
}

#[test]
fn ctrl_alt_hjkl_takes_the_cursor_corner_to_the_next_picture_edge() {
    let (mut s, asked) = with_edges();
    s.keys("<C-A-l>");
    // From the pixel just ahead of the edge, within its monitor; the edge
    // stops just before the change.
    assert_eq!(
        asked.borrow()[0],
        (PixelPoint::new(500, 399), PixelRect::new(0, 0, 1920, 1080))
    );
    assert_eq!(s.rect(), r(100.0, 100.0, 450.0, 300.0));
    // Again: on from there.
    s.keys("<C-A-l>");
    assert_eq!(asked.borrow()[1].0, PixelPoint::new(550, 399));
    assert_eq!(s.rect(), r(100.0, 100.0, 500.0, 300.0));
    // Back the other way, and down.
    s.keys("<C-A-h> <C-A-j>");
    assert_eq!(s.rect(), r(100.0, 100.0, 450.0, 350.0));
    // One undo step each.
    s.keys("u");
    assert_eq!(s.rect(), r(100.0, 100.0, 450.0, 300.0));
}

#[test]
fn in_centre_mode_picture_edges_move_the_cursor_edge_mirrored() {
    // The cursor's edges are the left and top, as for h and w: l shrinks,
    // h grows, both about the centre.
    let (mut s, asked) = with_edges();
    s.keys("c <C-A-l>");
    assert_eq!(asked.borrow()[0].0, PixelPoint::new(100, 100));
    assert_eq!(s.rect(), r(150.0, 100.0, 300.0, 300.0));
    assert!(s.selection().is_centred());
    s.keys("<C-A-h>");
    assert_eq!(asked.borrow()[1].0, PixelPoint::new(149, 100));
    assert_eq!(s.rect(), r(100.0, 100.0, 400.0, 300.0));
    s.keys("<C-A-j>");
    assert_eq!(s.rect(), r(100.0, 150.0, 400.0, 200.0));
    // The cursor is still on the left: l shrinks about the centre.
    s.keys("l");
    assert_eq!(s.rect(), r(105.0, 150.0, 390.0, 200.0));
}

#[test]
fn with_no_edge_that_way_the_box_stays_and_says_so() {
    let (mut s, _) = with_edges();
    s.keys("<C-A-k>");
    assert_eq!(s.rect(), r(100.0, 100.0, 400.0, 300.0));
    assert!(s.status().contains("no edge"), "{}", s.status());
}

#[test]
fn the_low_corner_lands_on_the_edge_too() {
    let (mut s, asked) = with_edges();
    s.keys("o <C-A-h>"); // the cursor to the top-left; then left
    assert_eq!(asked.borrow()[0].0, PixelPoint::new(99, 100));
    assert_eq!(s.rect(), r(50.0, 100.0, 450.0, 300.0));
}

#[test]
fn censor_filter_and_shape_boxes_jump_to_picture_edges_too() {
    for tool in ["x", "f", "a r"] {
        let (mut s, asked) = with_edges();
        s.keys(&format!("{tool} <C-A-h>"));
        assert_eq!(asked.borrow().len(), 1, "{tool}");
    }
}
