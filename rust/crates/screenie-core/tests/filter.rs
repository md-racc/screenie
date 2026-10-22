//! Filter: the stack, amounts, the list keys and its undo.

mod common;

use common::*;
use screenie_core::filter::describe;
use screenie_core::model::{FilterKind, FilterOp, OpData};
use screenie_core::session::{Button, Mode, Pending, Session};

fn stack(s: &Session) -> Vec<FilterOp> {
    s.filter_stack()
        .iter()
        .map(|&id| *s.filter(id).unwrap())
        .collect()
}

fn kinds(s: &Session) -> Vec<FilterKind> {
    stack(s).iter().map(|f| f.kind).collect()
}

fn selected(s: &Session) -> Option<FilterOp> {
    match s.modes().last() {
        Some(Mode::Filter(f)) => f.selected.map(|id| *s.filter(id).unwrap()),
        _ => panic!("not in filter: {}", s.mode_name()),
    }
}

fn selected_index(s: &Session) -> Option<usize> {
    let Some(Mode::Filter(f)) = s.modes().last() else {
        panic!()
    };
    s.filter_stack()
        .iter()
        .position(|&id| Some(id) == f.selected)
}

fn sel_describe(s: &Session) -> String {
    describe(&selected(s).unwrap())
}

use FilterKind::*;

#[test]
fn filter_keys_pick_the_filter_and_plus_minus_adjust_it() {
    let mut s = session();
    s.keys("f");
    assert_eq!(s.mode_name(), "filter");
    assert!(stack(&s).is_empty());
    assert!(s.status().ends_with("  layer  raw"), "{}", s.status());
    s.keys("plus"); // nothing to adjust yet
    assert_eq!(s.note(), Some("pick a filter: 1-5"));
    assert_eq!(
        s.palette(),
        ["1 grey  2 invert  3 bright  4 contrast  5 blur"]
    );
    s.keys("1 minus");
    assert_eq!(sel_describe(&s), "grey 75%");
    s.keys("3 minus minus minus"); // a second filter, on top
    assert_eq!(sel_describe(&s), "brightness -2");
    s.keys(&format!("4 {}", "plus ".repeat(9)));
    assert_eq!(sel_describe(&s), "contrast +4"); // capped
    s.keys("5 plus");
    assert_eq!(sel_describe(&s), "blur 6px");
    s.keys("2 plus"); // invert has no amount
    assert_eq!(sel_describe(&s), "invert");
    assert_eq!(kinds(&s), [Grey, Brightness, Contrast, Blur, Invert]);
    s.keys(&"h ".repeat(20)); // the box reshapes the selected filter only
    assert!(selected(&s).unwrap().rect.w < START.w);
    assert_eq!(stack(&s)[0].rect, START);
    s.keys("Return"); // the whole stack, in one layer
    assert_eq!(s.layers().len(), 1);
    assert_eq!(s.layers()[0].ops.len(), 5);
}

#[test]
fn new_filters_go_just_above_the_selected_one() {
    let mut s = session();
    s.keys("f 1 3 bracketleft"); // select grey (the bottom)
    s.keys("2"); // invert lands between grey and brightness
    assert_eq!(kinds(&s), [Grey, Invert, Brightness]);
    assert_eq!(selected(&s).unwrap().kind, Invert); // and is selected
    s.keys("f"); // a duplicate goes just above it too
    assert_eq!(kinds(&s), [Grey, Invert, Invert, Brightness]);
    assert_eq!(selected_index(&s), Some(2));
}

#[test]
fn select_move_mute_duplicate_delete_and_undo_them() {
    let mut s = session();
    s.keys("f 1 3 2");
    assert_eq!(selected_index(&s), Some(2));
    s.keys("bracketleft");
    assert_eq!(selected(&s).unwrap().kind, Brightness);
    s.keys("greater"); // brightness up above invert
    assert_eq!(kinds(&s), [Grey, Invert, Brightness]);
    assert_eq!(selected(&s).unwrap().kind, Brightness); // selection follows it
    s.keys("space");
    assert!(selected(&s).unwrap().muted);
    s.keys("space"); // unmuted: it applies again
    assert!(!selected(&s).unwrap().muted);
    s.keys("f"); // duplicate it, just above it (on top here)
    assert_eq!(stack(&s).len(), 4);
    let dup = s.filter_stack()[3];
    assert_ne!(dup, s.filter_stack()[2]); // a filter of its own
    s.keys("d");
    assert_eq!(stack(&s).len(), 3);
    s.keys("u"); // the delete, back in its place
    assert_eq!(s.filter_stack()[3], dup);
    s.keys("u"); // then the duplicate
    assert_eq!(stack(&s).len(), 3);
    s.keys("u"); // then the unmute
    assert!(stack(&s)[2].muted);
}

#[test]
fn undo_is_the_last_action_anywhere_in_the_stack_and_a_run_of_plus_is_one() {
    let mut s = session();
    s.keys("f 1 2 bracketleft 3"); // brightness goes mid-stack
    s.keys("plus plus");
    assert_eq!(selected(&s).unwrap().amount, 3.0);
    s.keys("u");
    assert_eq!(selected(&s).unwrap().amount, 1.0);
    assert_eq!(kinds(&s), [Grey, Brightness, Invert]);
    s.keys("u"); // the insert, mid-stack, not the top
    assert_eq!(kinds(&s), [Grey, Invert]);
}

#[test]
fn deleting_selects_the_next_row_down_else_the_one_up() {
    let mut s = session();
    s.keys("f 1 2 3"); // listed top first: brightness, invert, grey
    s.keys("d"); // brightness (top): the row below it
    assert_eq!(selected(&s).unwrap().kind, Invert);
    s.keys("bracketleft d"); // grey (bottom): the row above it
    assert_eq!(selected(&s).unwrap().kind, Invert);
    s.keys("d"); // the last one
    assert_eq!(selected(&s), None);
    assert!(s.status().ends_with("raw"));
}

#[test]
fn the_list_and_the_status_show_the_stack() {
    let mut s = session();
    s.keys("f 1 3 bracketleft");
    assert_eq!(
        s.lists(),
        [vec![
            "  brightness +1".to_string(),
            "> grey 100%".to_string()
        ]]
    );
    assert!(
        s.status().ends_with("  layer +2  1/2 grey 100%  560×90"),
        "{}",
        s.status()
    );
}

#[test]
fn the_palette_is_always_shown_and_not_in_the_legend() {
    let mut s = session();
    s.keys("f");
    assert!(s.palette()[0].contains("5 blur"));
    assert!(!s.legend().join(" ").contains("5 blur"));
}

#[test]
fn the_filter_box_starts_as_the_whole_main_box() {
    let mut s = session();
    s.keys("f");
    let Some(Mode::Filter(f)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(f.sel.rect(), START);
    s.keys("<S-space>"); // Shift+Space is still the sideways corner
    let Some(Mode::Filter(f)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(f.sel.cursor_point(), p(START.x, START.bottom()));
}

#[test]
fn a_drag_reshapes_the_selected_filter_as_one_undo_step() {
    let mut s = session();
    s.keys("f 1 drag:200,180:400,240");
    assert_eq!(selected(&s).unwrap().rect, r(200.0, 180.0, 200.0, 60.0));
    s.keys("u");
    assert_eq!(selected(&s).unwrap().rect, START);
    let Some(Mode::Filter(f)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(f.sel.rect(), START); // the box follows
}

#[test]
fn tools_share_one_layer_filters_included() {
    let mut s = session();
    s.keys("a 5 drag:370,205:390,225");
    s.keys("f"); // straight to the filter tool, same layer
    assert_eq!(s.mode_name(), "filter");
    s.keys("1 f"); // a grey over it, and a duplicate
    s.keys("x"); // to Censor
    assert_eq!(s.mode_name(), "censor");
    let ops: Vec<&str> = s
        .open_layer()
        .unwrap()
        .ops
        .iter()
        .map(|o| o.data.noun())
        .collect();
    assert_eq!(ops, ["pen", "filter", "filter"]);
    s.keys("u"); // drops the last op, whichever tool made it
    assert_eq!(s.open_layer().unwrap().ops.len(), 2);
    s.keys("Return"); // locks the censor, commits the one layer
    assert_eq!((s.mode_name(), s.layers().len()), ("select", 1));
    assert_eq!(s.layers()[0].ops.len(), 3);
    assert!(matches!(s.layers()[0].ops[2].data, OpData::Censor(_)));
}

#[test]
fn esc_warns_when_filters_are_in_the_layer() {
    let mut s = session();
    s.keys("f 1 Escape");
    assert_eq!(s.pending(), Some(Pending::Discard));
    s.keys("Escape");
    assert_eq!((s.mode_name(), s.layers().len()), ("select", 0));
}

#[test]
fn no_stale_drag_under_a_lens() {
    let rect = |s: &Session| match s.modes().last() {
        Some(Mode::Filter(f)) => f.sel.rect(),
        _ => panic!("not in filter: {}", s.mode_name()),
    };
    let mut s = session();
    s.keys("f");
    s.button(Button::Left, p(200.0, 200.0), true);
    s.motion(p(250.0, 230.0));
    let drawn = rect(&s);
    s.keys("e"); // the drag ends here
    s.button(Button::Left, p(250.0, 230.0), false); // the release goes to Enhance
    s.keys("Escape");
    s.motion(p(500.0, 250.0)); // no button held
    assert_eq!(rect(&s), drawn);
}
