//! The Layers view: picking, moving, deleting and hiding committed layers,
//! and their project-history undo. Ported from the prototype's
//! tests/test_modes.py and test_audit.py (at the `python-prototype` tag),
//! following README where the prototype differs: hiding is an undo step,
//! and `u` undoes the last layer action, not a box change.

mod common;

use common::*;
use screenie_core::model::LayerId;
use screenie_core::session::{Mode, Session};

/// Three layers: a censor, a filter, a censor.
fn three() -> Session {
    let mut s = session();
    s.keys("x Return f 1 Return x Return");
    assert_eq!(s.layers().len(), 3);
    s
}

fn ids(s: &Session) -> Vec<LayerId> {
    s.layers().iter().map(|l| l.id).collect()
}

fn selected(s: &Session) -> LayerId {
    match s.modes().last() {
        Some(Mode::Layers(m)) => m.selected.unwrap(),
        _ => panic!("not in layers: {}", s.mode_name()),
    }
}

#[test]
fn no_layers_no_view() {
    let mut s = session();
    s.keys("1");
    assert_eq!((s.mode_name(), s.note()), ("select", Some("no layers yet")));
}

#[test]
fn the_layer_view() {
    let mut s = three();
    let first = s.layers()[0].id;
    s.keys("2");
    assert_eq!(s.mode_name(), "layers");
    assert_eq!(selected(&s), s.layers()[1].id);
    assert!(s.status().ends_with("  2/3  filter"), "{}", s.status());
    s.keys("bracketleft"); // the layer below
    assert_eq!(selected(&s), first);
    s.keys("greater"); // move it up: it now applies second
    assert_eq!(s.layers()[1].id, first);
    assert_eq!(selected(&s), first); // selection follows it
    s.keys("space"); // hide it: out of the picture, still listed
    assert!(s.layers()[1].hidden);
    assert!(s.status().ends_with("hidden"));
    s.keys("space");
    s.keys("d"); // delete it
    assert_eq!(s.layers().len(), 2);
    assert!(!ids(&s).contains(&first));
    s.keys("Escape");
    assert_eq!(s.mode_name(), "select");
    // In Select, u walks back through all of it: hiding is an undo step.
    s.keys("u"); // the delete, back in its place
    assert_eq!(s.layers()[1].id, first);
    s.keys("u u"); // the unhide, then the hide
    assert!(!s.layers()[1].hidden);
    s.keys("u"); // the move
    assert_eq!(s.layers()[0].id, first);
}

#[test]
fn u_in_layers_undoes_the_last_layer_action_not_a_box_change() {
    let mut s = three();
    s.keys("l"); // a box change after the layers
    s.keys("3 d"); // delete the top layer
    assert_eq!(s.layers().len(), 2);
    s.keys("l"); // (nothing: the pen has no box, nor does Layers)
    s.keys("u");
    assert_eq!(s.layers().len(), 3);
    s.keys("u"); // the last commit, not the box change
    assert_eq!(s.layers().len(), 2);
    assert_eq!(s.rect().w, START.w + 5.0); // the box change stands
}

#[test]
fn deleting_the_last_layer_backs_out_and_select_u_brings_it_back() {
    let mut s = session();
    s.keys("x Return 1 d");
    assert_eq!(s.mode_name(), "select"); // the last layer: back out
    s.keys("1"); // nothing to view
    assert_eq!(s.mode_name(), "select");
    s.keys("u"); // Select's u brings it back
    assert_eq!(s.layers().len(), 1);
}

#[test]
fn digits_plus_minus_and_the_wheel_pick_layers() {
    let mut s = three();
    let all = ids(&s);
    s.keys("1");
    assert_eq!(selected(&s), all[0]);
    s.keys("plus plus plus"); // stops at the top
    assert_eq!(selected(&s), all[2]);
    s.keys("wheel:-1");
    assert_eq!(selected(&s), all[1]);
    s.keys("9"); // past the end: the top
    assert_eq!(selected(&s), all[2]);
}

#[test]
fn a_click_picks_the_topmost_layer_under_the_pointer() {
    let mut s = session();
    s.keys("x Return"); // a censor at the centre (370..390, 205..225)
    s.keys("f 1 Return"); // a filter over the whole box
    s.keys("2 click:380,215");
    assert_eq!(selected(&s), s.layers()[1].id); // the filter is on top
    s.keys("click:150,180"); // only the filter here
    assert_eq!(selected(&s), s.layers()[1].id);
    s.keys("1 click:1000,500"); // no layer there: unchanged
    assert_eq!(selected(&s), s.layers()[0].id);
}

#[test]
fn the_list_shows_every_layer_top_first() {
    let mut s = three();
    s.keys("2 space");
    assert_eq!(
        s.lists(),
        [vec![
            "  3  censor".to_string(),
            "> 2  filter  (hidden)".to_string(),
            "  1  censor".to_string(),
        ]]
    );
    assert_eq!(s.inspected().map(|l| l.id), Some(s.layers()[1].id));
}

#[test]
fn enter_and_esc_go_back_to_select() {
    for key in ["Return", "Escape", "rclick"] {
        let mut s = three();
        s.keys(&format!("1 {key}"));
        assert_eq!(s.mode_name(), "select", "{key}");
        assert_eq!(s.outcome(), None, "{key}");
    }
}
