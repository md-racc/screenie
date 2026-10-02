//! The picker's command-line options that live in core: `--result`,
//! `--mode` and `--lock-box` (docs/cli.md, PICK OPTIONS).

mod common;

use common::*;
use screenie_core::select::{EnterResult, StartMode};
use screenie_core::session::{Button, FinishedAction, Outcome};

fn done(s: &screenie_core::session::Session) -> &FinishedAction {
    match s.outcome() {
        Some(Outcome::Done(a)) => a,
        other => panic!("{other:?}"),
    }
}

#[test]
fn result_says_what_enter_produces() {
    let mut s = session();
    s.set_result(EnterResult::Text);
    s.keys("Return");
    assert_eq!(done(&s), &FinishedAction::Ocr { rect: START });

    let mut s = session();
    s.set_result(EnterResult::Colour);
    s.keys("Return");
    assert!(matches!(done(&s), FinishedAction::Colour { .. }));

    let mut s = session();
    s.set_result(EnterResult::Pin);
    s.keys("Return");
    assert_eq!(done(&s), &FinishedAction::Pin { rect: START });

    // The result keys still work.
    let mut s = session();
    s.set_result(EnterResult::Text);
    s.keys("P");
    assert_eq!(done(&s), &FinishedAction::Pin { rect: START });
}

#[test]
fn result_measure_opens_measure_where_enter_delivers() {
    let mut s = session();
    s.set_result(EnterResult::Measure);
    s.keys("Return");
    assert_eq!(s.mode_name(), "measure");
    s.keys("space j j Return Escape Return");
    assert!(matches!(done(&s), FinishedAction::Measure { .. }));
}

#[test]
fn mode_opens_there_as_if_its_key_had_been_pressed() {
    for (mode, name) in [
        (StartMode::Select, "select"),
        (StartMode::Insert, "insert"),
        (StartMode::Censor, "censor"),
        (StartMode::Annotate, "annotate"),
        (StartMode::Filter, "filter"),
        (StartMode::Measure, "measure"),
    ] {
        let mut s = session();
        s.start_in(mode);
        assert_eq!(s.mode_name(), name);
    }
    let mut s = session();
    s.start_in(StartMode::Censor);
    assert!(s.open_layer().is_some(), "a build tool opens a layer");
}

#[test]
fn a_locked_box_stays_put_and_its_tools_still_work() {
    let mut s = session();
    s.lock_box();
    s.keys("l <S-l> <C-l> w plus period c");
    assert_eq!(s.rect(), START);
    assert!(s.status().contains("the box is locked"), "{}", s.status());
    s.button(Button::Left, p(1000.0, 500.0), true);
    s.button(Button::Left, p(1000.0, 500.0), false);
    assert_eq!(s.rect(), START);
    s.keys("x l Return Return");
    assert_eq!(s.layers().len(), 1);
    assert_eq!(done(&s), &FinishedAction::Image { rect: START });
}
