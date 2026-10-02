//! Measure (`m` in Select): primitives, points, calibration, the list and
//! its totals, output and baking (docs/measure.md).

mod common;

use common::*;
use screenie_core::geometry::{Dir, PixelPoint, PixelRect};
use screenie_core::measure::{EdgeFinder, MeasureMode, Prim};
use screenie_core::model::OpData;
use screenie_core::session::{FinishedAction, Mode, Outcome, Session};

fn measure(s: &Session) -> &MeasureMode {
    match s.modes().last() {
        Some(Mode::Measure(m)) => m,
        _ => panic!("not in measure: {}", s.mode_name()),
    }
}

fn at(s: &Session) -> (i32, i32) {
    let c = measure(s).cursor.at;
    (c.x, c.y)
}

fn labels(s: &Session) -> Vec<String> {
    s.measurements()
        .unwrap()
        .items
        .iter()
        .map(|i| i.label.clone())
        .collect()
}

/// A 100px line to the right of where the cursor starts: 20 steps of 5.
const LINE: &str = "space l l l l l l l l l l l l l l l l l l l l space Return";

#[test]
fn m_opens_measure_at_the_box_cursor_corner_on_its_monitor() {
    let mut s = session();
    s.keys("m");
    assert_eq!(s.mode_name(), "measure");
    assert_eq!(at(&s), (659, 259)); // the cursor corner's pixel
    let m = measure(&s);
    assert_eq!(m.tool, Prim::Line);
    assert_eq!(m.cursor.bounds, PixelRect::new(0, 0, 1920, 1080)); // its monitor
    assert!(s.status().contains("line  (659,259)"), "{}", s.status());
    s.keys("<S-$>");
    assert_eq!(at(&s), (1919, 259)); // its monitor's edge, not the next one's
}

#[test]
fn the_first_line_asks_for_its_length_and_sets_the_scale() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
    assert_eq!(s.mode_name(), "text"); // = 5 m
    assert!(s.status().contains("Enter sets the scale"));
    s.keys("2 space m Return");
    assert_eq!(s.mode_name(), "measure");
    assert!(s.status().contains("1 m = 50.0 px ±1.4%"), "{}", s.status());
    assert_eq!(labels(&s), ["ref 2.00 m ±0.04"]);
    // The next line reads in metres; the live length too.
    s.keys("space k k k k k");
    assert!(s.status().contains("→ 0.500 m"), "{}", s.status());
    s.keys("Return");
    assert_eq!(s.mode_name(), "measure"); // asks only once
    assert_eq!(labels(&s)[1], "0.500 m ±0.029");
}

#[test]
fn esc_at_the_question_stays_in_pixels_and_c_asks_again() {
    let mut s = session();
    s.keys(&format!("m {LINE} Escape"));
    assert_eq!(labels(&s), ["100.0 px"]);
    s.keys(LINE);
    assert_eq!(s.mode_name(), "measure"); // not asked again
    s.keys("C");
    assert_eq!(s.mode_name(), "text"); // for the selected one
    s.keys("1 0 space c m Return");
    assert!(s.status().contains("1 cm = 10.0 px"), "{}", s.status());
}

#[test]
fn a_bad_length_says_why() {
    let mut s = session();
    s.keys(&format!("m {LINE} 5 space p a r s e c Return"));
    assert!(s.status().contains("can't read"), "{}", s.status());
    assert_eq!(labels(&s), ["100.0 px"]);
}

#[test]
fn arcs_circles_and_angles_finish_at_three_points() {
    let mut s = session();
    s.keys("m ( space l l space j j space");
    assert_eq!(s.mode_name(), "text"); // an arc can be the reference
    s.keys("Escape");
    assert_eq!(measure(&s).items.len(), 1);
    assert_eq!(measure(&s).items[0].kind, Prim::Arc);
    s.keys("^ space h h space k k space");
    assert_eq!(labels(&s)[1], "90.0°");
    s.keys("c space l l space j j space");
    assert!(labels(&s)[2].starts_with("Ø "), "{:?}", labels(&s));
}

#[test]
fn r_then_enter_measures_the_box() {
    let mut s = session();
    s.keys("m r Return");
    assert_eq!(labels(&s), ["50400 px²"]);
    let v = measure(&s).items[0].values();
    assert_eq!((v.width, v.height), (Some(560.0), Some(90.0)));
}

#[test]
fn areas_need_three_points_and_enter() {
    let mut s = session();
    s.keys("m A space l l space Return");
    assert!(s.status().contains("needs 3 points"), "{}", s.status());
    s.keys("j j space Return");
    assert_eq!(labels(&s), ["50 px²"]);
}

#[test]
fn u_takes_back_a_point_then_a_measurement_and_its_scale() {
    let mut s = session();
    s.keys(&format!("m {LINE} 1 space m Return"));
    s.keys("space l u");
    assert!(measure(&s).points.is_empty());
    assert_eq!(at(&s), (764, 259)); // the cursor stays
    s.keys("u");
    assert!(measure(&s).items.is_empty());
    assert!(measure(&s).scale.is_none(), "the reference went with it");
}

#[test]
fn points_snap_to_points_unless_alt() {
    let mut s = session();
    s.keys(&format!("m {LINE} Escape"));
    let end = measure(&s).items[0].points[1];
    let near = p(f64::from(end.x) + 2.5, f64::from(end.y) + 2.5); // 2px off
    s.motion(p(0.0, 0.0)); // where the pointer was when Measure opened
    s.motion(near);
    s.keys("space");
    assert_eq!(measure(&s).points[0], end);
    s.keys("Escape");
    s.motion(p(0.0, 0.0));
    s.motion(near);
    s.keys("<A-space>");
    assert_ne!(measure(&s).points[0], end);
}

#[test]
fn enter_ends_a_line_at_the_cursor() {
    let mut s = session();
    s.keys("m space j j Return Escape");
    let v = measure(&s).items[0].values();
    assert_eq!(v.length, Some(10.0));
}

struct Edges;

impl EdgeFinder for Edges {
    fn edge(&mut self, _: &Session, from: PixelPoint, d: Dir, _: PixelRect) -> Option<PixelPoint> {
        // A wall at x = 900, nothing else.
        (d == Dir::Right && from.x < 900).then(|| PixelPoint::new(900, from.y))
    }
}

#[test]
fn ctrl_hjkl_jumps_to_the_next_edge_in_the_picture() {
    let mut s = session();
    s.set_edge_finder(Box::new(Edges));
    s.keys("m <C-l>");
    assert_eq!(at(&s), (900, 259));
    s.keys("<C-l>");
    assert_eq!(at(&s), (900, 259));
    assert!(s.status().contains("no edge right"), "{}", s.status());
}

#[test]
fn t_switches_system_and_capital_t_shows_pixels() {
    let mut s = session();
    s.keys(&format!("m {LINE} 1 space m Return"));
    s.keys("t");
    assert!(labels(&s)[0].starts_with("ref 3.28 ft"), "{:?}", labels(&s));
    s.keys("T");
    assert_eq!(labels(&s), ["ref 100.0 px"]);
    s.keys("T");
    assert!(labels(&s)[0].starts_with("ref 1.00 m"), "{:?}", labels(&s));
}

#[test]
fn the_list_totals_what_is_counted() {
    let mut s = session();
    s.keys(&format!("m {LINE} Escape {LINE} {LINE}"));
    let rows = s.lists().pop().unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows[2].starts_with("> L3"), "{rows:?}");
    assert_eq!(rows[3], "  total 300.0 px");
    s.keys("x");
    let rows = s.lists().pop().unwrap();
    assert!(rows[2].ends_with("(not in total)"));
    assert_eq!(rows[3], "  total 200.0 px");
    s.keys("bracketleft d");
    assert_eq!(measure(&s).items.len(), 2);
}

#[test]
fn y_delivers_the_measurements_as_text_and_data() {
    let mut s = session();
    s.keys(&format!("m {LINE} 5 space m Return"));
    s.keys("space j j j j j j j j j j Return y");
    let Some(Outcome::Done(FinishedAction::Measure { text, json })) = s.outcome() else {
        panic!("{:?}", s.outcome());
    };
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "ref   5.00 m  (100.0 px)");
    assert!(lines[1].starts_with("L2    2.50 m ±0.0"), "{text}");
    assert!(lines[1].ends_with("at -90.0°"), "{text}");
    assert_eq!(json["scale"]["px"], 100.0);
    assert_eq!(json["measurements"][1]["real"], 2.5);
    assert_eq!(json["measurements"][1]["unit"], "m");
    assert_eq!(json["measurements"][0]["reference"], true);
}

#[test]
fn a_bakes_them_into_a_layer_and_enter_then_captures() {
    let mut s = session();
    s.keys(&format!(
        "m {LINE} Escape A space l l space j j space Return a"
    ));
    assert_eq!(s.mode_name(), "select");
    let ops = &s.layers()[0].ops;
    assert!(matches!(ops[0].data, OpData::Line(_))); // a dimension line
    assert!(matches!(&ops[1].data, OpData::Text(t) if t.text == "100.0 px"));
    assert!(matches!(ops[2].data, OpData::Stroke(_))); // the area's outline
    s.keys("u");
    assert!(s.layers().is_empty(), "one undo takes the bake back");
}

#[test]
fn esc_warns_before_dropping_measurements_and_the_scale_is_kept() {
    let mut s = session();
    s.keys(&format!("m {LINE} 1 space m Return Escape"));
    assert_eq!(s.mode_name(), "measure");
    assert!(s.status().contains("Esc again drops 1"), "{}", s.status());
    s.keys("Escape");
    assert_eq!(s.mode_name(), "select");
    s.keys("m");
    assert!(measure(&s).items.is_empty());
    assert!(s.status().contains("1 m = 100.0 px"), "the scale stays");
}

#[test]
fn the_loupe_hands_its_position_back() {
    let mut s = session();
    s.keys("m z l l Return");
    assert_eq!(s.mode_name(), "measure");
    assert_eq!(at(&s), (661, 259));
}

#[test]
fn the_primitive_keys_switch_and_drop_the_points_in_progress() {
    let mut s = session();
    s.keys("m A space l");
    assert_eq!(measure(&s).tool, Prim::Area);
    s.keys("s");
    assert_eq!(measure(&s).tool, Prim::Line);
    assert!(measure(&s).points.is_empty());
    s.keys("question");
    assert!(
        s.legend().join(" ").contains("s line/path"),
        "{:?}",
        s.legend()
    );
}

#[test]
fn a_point_on_the_last_one_is_refused() {
    let mut s = session();
    s.keys("m space space");
    assert_eq!(measure(&s).points.len(), 1);
    assert!(
        s.status().contains("move the cursor first"),
        "{}",
        s.status()
    );
    s.keys("Return");
    assert!(measure(&s).items.is_empty(), "no zero-length line");
}

#[test]
fn the_pointer_moves_the_cursor_only_once_it_moves() {
    let mut s = session();
    s.keys("m");
    s.motion(p(1500.0, 900.0)); // the overlay appeared under it
    assert_eq!(at(&s), (659, 259));
    s.motion(p(1500.0, 901.0));
    assert_eq!(at(&s), (1500, 901));
}
