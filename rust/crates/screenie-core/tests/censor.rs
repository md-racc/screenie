//! Censor, the open layer, its undo, and the Esc / q warnings. Ported
//! from the prototype's tests/test_modes.py and tests/test_audit.py.

mod common;

use common::*;
use screenie_core::keys::Mods;
use screenie_core::model::{CensorKind, OpData};
use screenie_core::session::{Button, FinishedAction, Mode, Outcome, Pending, Session};

fn censor(s: &Session) -> &screenie_core::censor::CensorMode {
    match s.modes().last() {
        Some(Mode::Censor(c)) => c,
        _ => panic!("not in censor: {}", s.mode_name()),
    }
}

fn describe(s: &Session) -> String {
    censor(s).describe()
}

fn kinds(ops: &[screenie_core::model::Op]) -> Vec<CensorKind> {
    ops.iter()
        .map(|o| match &o.data {
            OpData::Censor(c) => c.kind,
            other => panic!("not a censor: {other:?}"),
        })
        .collect()
}

#[test]
fn plus_minus_set_pixelation_and_shade_each_kind_its_own() {
    let mut s = session();
    s.keys("x");
    assert_eq!(describe(&s), "pixelate 10px"); // the defaults
    s.keys("plus");
    assert_eq!(describe(&s), "pixelate 12px");
    s.keys(&"minus ".repeat(30));
    assert_eq!(describe(&s), "pixelate 4px"); // the floor
    s.keys("t");
    assert_eq!(describe(&s), "black bar");
    s.keys("plus plus plus");
    assert_eq!(describe(&s), "grey bar 30%");
    s.keys(&"plus ".repeat(20));
    assert_eq!(describe(&s), "white bar");
    s.keys("x"); // the next keeps its type and shade
    assert_eq!(censor(&s).amount(), 1.0);
    s.keys("t");
    assert_eq!(describe(&s), "pixelate 4px"); // each type kept its own
}

#[test]
fn it_starts_as_a_text_height_square_at_the_centre() {
    let mut s = session();
    s.keys("x");
    let r = censor(&s).sel.rect();
    assert_eq!((r.w, r.h), (20.0, 20.0));
    assert_eq!(r.centre(), START.centre());
    assert_eq!(censor(&s).sel.bounds(), START);
}

#[test]
fn the_censor_box_lives_inside_the_main_box() {
    let mut s = session();
    s.keys("x");
    s.keys(&"L ".repeat(100));
    assert!(censor(&s).sel.rect().right() <= START.right());
    assert_eq!(s.rect(), START); // the main box is frozen
}

#[test]
fn enter_commits_and_u_in_select_takes_the_layer_back() {
    let mut s = session();
    s.keys("x t Return");
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers().len(), 1);
    assert_eq!(kinds(&s.layers()[0].ops), [CensorKind::Bar]);
    assert_eq!(s.layers()[0].clip, START);
    assert!(s.open_layer().is_none());
    s.keys("u");
    assert!(s.layers().is_empty());
    assert_eq!(s.rect(), START);
}

#[test]
fn x_locks_and_starts_the_next_and_enter_commits_them_all() {
    let mut s = session();
    s.keys("x");
    let first = censor(&s).sel.rect();
    s.keys("x"); // lock the pixelate, start the next
    assert_eq!(s.mode_name(), "censor");
    assert_eq!(s.open_layer().unwrap().ops.len(), 1);
    assert_eq!(censor(&s).sel.rect(), first); // the next starts where that one was
    assert!(s.layers().is_empty()); // locked, not committed yet
    s.keys("t");
    s.keys(&"L ".repeat(8));
    s.keys("x");
    assert_eq!(
        kinds(&s.open_layer().unwrap().ops),
        [CensorKind::Pixelate, CensorKind::Bar]
    );
    s.keys("u"); // in the tool: drops the last locked censor
    assert_eq!(s.open_layer().unwrap().ops.len(), 1);
    s.keys("Return"); // locks the current one, commits the layer
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers().len(), 1);
    assert_eq!(s.layers()[0].ops.len(), 2);
    s.keys("u"); // in Select: the whole layer
    assert!(s.layers().is_empty());
}

#[test]
fn undo_in_a_tool_never_reaches_project_history() {
    let mut s = session();
    s.keys("l x u u u");
    assert_eq!(s.mode_name(), "censor");
    assert_eq!(s.rect().w, START.w + 5.0); // the step in Select stands
}

#[test]
fn esc_twice_throws_locked_work_away() {
    let mut s = session();
    s.keys("x x");
    s.keys("Escape"); // there's work in it: only a warning
    assert_eq!(s.mode_name(), "censor");
    assert_eq!(s.pending(), Some(Pending::Discard));
    assert!(
        s.status().contains("Esc again to discard 1"),
        "{}",
        s.status()
    );
    s.keys("l"); // any other key disarms it
    assert_eq!(s.pending(), None);
    s.keys("Escape");
    s.button(Button::Middle, p(500.0, 500.0), true); // the middle click is Esc too
    assert_eq!(s.mode_name(), "select");
    assert!(s.open_layer().is_none());
    assert!(s.layers().is_empty());
    assert_eq!(s.outcome(), None);
}

#[test]
fn esc_with_nothing_locked_leaves_at_once() {
    let mut s = session();
    s.keys("x Escape");
    assert_eq!(s.mode_name(), "select");
    assert!(s.open_layer().is_none());
}

#[test]
fn new_work_takes_the_discard_warning_back() {
    let mut s = session();
    s.keys("x x Escape");
    assert_eq!(s.pending(), Some(Pending::Discard));
    s.keys("x"); // another censor locked
    assert_eq!(s.pending(), None);
    s.button(Button::Middle, p(0.0, 0.0), true); // Esc: only warns again
    assert_eq!(
        (s.mode_name(), s.open_layer().unwrap().ops.len()),
        ("censor", 2)
    );
}

#[test]
fn a_drag_draws_the_censor_box_and_never_finishes() {
    let mut s = session();
    s.keys("x drag:150,180:300,240");
    assert_eq!(censor(&s).sel.rect(), r(150.0, 180.0, 150.0, 60.0));
    assert_eq!(s.outcome(), None);
}

#[test]
fn the_censor_box_drags_by_its_edges_too() {
    let mut s = session();
    s.keys("x drag:150,180:300,240 drag:300,210:350,210");
    assert_eq!(censor(&s).sel.rect(), r(150.0, 180.0, 200.0, 60.0));
}

#[test]
fn a_click_places_the_cursor_corner() {
    let mut s = session();
    s.keys("x click:400,240");
    let b = censor(&s).sel.rect();
    assert_eq!((b.right(), b.bottom()), (400.0, 240.0)); // the corner, not the whole box
    assert!(b.w < START.w);
}

#[test]
fn a_drag_past_the_bounds_stops_at_the_edge() {
    let mut s = session();
    s.keys("x");
    s.button(Button::Left, p(600.0, 200.0), true);
    s.motion(p(700.0, 230.0));
    assert_eq!(censor(&s).sel.rect(), r(600.0, 200.0, 60.0, 30.0));
}

#[test]
fn right_click_commits_in_a_tool_and_captures_in_select() {
    let mut s = session();
    s.keys("x rclick");
    assert_eq!((s.mode_name(), s.layers().len()), ("select", 1));
    assert_eq!(s.outcome(), None);
    s.keys("rclick");
    assert_eq!(
        s.outcome(),
        Some(&Outcome::Done(FinishedAction::Image { rect: START }))
    );
}

#[test]
fn q_quits_from_censor_and_with_locked_work_asks_twice() {
    let mut s = session();
    s.keys("x q");
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
    let mut s = session();
    s.keys("x x q");
    assert_eq!(s.outcome(), None); // only a warning
    assert!(
        s.status().contains("q again to quit, discarding 1"),
        "{}",
        s.status()
    );
    s.keys("t"); // any other key takes it back
    assert_eq!(s.pending(), None);
    s.keys("q q");
    assert_eq!(s.outcome(), Some(&Outcome::Cancelled));
}

#[test]
fn q_armed_then_a_right_click_commit_disarms() {
    let mut s = session();
    s.keys("x x q rclick");
    assert_eq!(s.pending(), None);
    assert_eq!((s.mode_name(), s.layers().len()), ("select", 1));
}

#[test]
fn the_status_line_in_censor() {
    let mut s = session();
    s.keys("x");
    assert_eq!(
        s.status(),
        "CENSOR  560×90  (389,224)  layer  pixelate 10px  20×20"
    );
    s.keys("x");
    assert!(
        s.status().contains("  layer +1  pixelate 10px  20×20"),
        "{}",
        s.status()
    );
}

#[test]
fn the_legend_in_censor() {
    let mut s = session();
    s.keys("x ?");
    assert!(s.show_help());
    assert!(
        s.legend()[0].starts_with("t pixelate/bar"),
        "{:?}",
        s.legend()
    );
}

#[test]
fn an_empty_box_opens_no_tool() {
    let mut s = session_at(r(100.5, 170.0, 0.6, 90.0), vec![]);
    s.keys("x");
    assert_eq!((s.mode_name(), s.note()), ("select", Some("empty box")));
}

#[test]
fn a_held_key_across_a_mode_change_is_not_a_new_press() {
    let mut s = session();
    s.keys("x");
    s.press(7, "h", Mods::NONE, 0.0); // held in Censor...
    s.keys("Escape");
    assert_eq!(s.mode_name(), "select");
    s.press(7, "h", Mods::NONE, 0.5); // ...its repeat arrives in Select
    assert!(!s.tick(1.0));
    assert_eq!(s.rect(), START);
    s.release(7, "h", Mods::NONE, 1.0);
    s.keys("h"); // a real press works again
    assert_ne!(s.rect(), START);
}

#[test]
fn no_stale_drag_after_a_mode_change() {
    let mut s = session();
    s.button(Button::Left, p(300.0, 300.0), true);
    s.motion(p(400.0, 400.0));
    s.keys("x"); // the drag in Select ends here
    s.button(Button::Left, p(400.0, 400.0), false); // the release goes to Censor
    s.keys("Escape");
    let before = s.rect();
    s.motion(p(900.0, 700.0)); // no button held
    assert_eq!(s.rect(), before);
}

#[test]
fn every_build_tool_chord_is_unique() {
    for (name, map) in screenie_core::keymap::all() {
        assert_eq!(map.collisions(), [], "{name}");
    }
}

#[test]
fn holding_plus_keeps_adjusting_at_the_keyboard_rate() {
    use screenie_core::session::RawEvent;
    let mut s = session();
    s.handle(
        RawEvent::RepeatInfo {
            rate: 10.0,
            delay: 0.5,
        },
        0.0,
    );
    s.keys("x");
    s.press(9, "plus", Mods::NONE, 1.0);
    assert_eq!(describe(&s), "pixelate 12px"); // the press
    s.tick(1.4);
    assert_eq!(describe(&s), "pixelate 12px"); // not yet
    s.tick(1.75); // 0.5s delay, then 10/s: three repeats by 0.75s
    assert_eq!(describe(&s), "pixelate 18px");
    s.release(9, "plus", Mods::NONE, 1.8);
    s.tick(3.0);
    assert_eq!(describe(&s), "pixelate 18px");
    // Other keys still fire once per press, however long they're held.
    s.press(10, "t", Mods::NONE, 4.0);
    s.tick(6.0);
    assert_eq!(describe(&s), "black bar");
}

#[test]
fn in_select_plus_does_not_repeat() {
    let mut s = session();
    s.press(9, "plus", Mods::NONE, 0.0);
    assert!(!s.tick(5.0));
    assert_eq!(s.rect(), WINDOWS[2]);
}
