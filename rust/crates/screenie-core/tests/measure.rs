//! Measure (`m` in Select): primitives, points, calibration, the list and
//! its totals, output and baking (docs/measure.md).

mod common;

use common::*;
use screenie_core::geometry::{Dir, PixelPoint, PixelRect};
use screenie_core::keys::Mods;
use screenie_core::measure::{EdgeFinder, MeasureMode, Prim};
use screenie_core::session::{Button, FinishedAction, Mode, Outcome, RawEvent, Session};

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

/// A 100px line to the right of where the cursor starts: 20 steps of 5; the
/// second point finishes it.
const LINE: &str = "space l l l l l l l l l l l l l l l l l l l l space";

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
fn c_asks_the_selected_lines_length_and_sets_the_scale() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
    assert_eq!(s.mode_name(), "measure"); // pixels until asked
    assert_eq!(labels(&s), ["100.0 px"]);
    s.keys("C"); // the line just drawn is selected
    assert_eq!(s.mode_name(), "text"); // = 5 m
    assert!(s.status().contains("Enter sets the scale"));
    s.keys("2 space m Return");
    assert_eq!(s.mode_name(), "measure");
    assert!(s.status().contains("1 m = 50.0 px ±1.4%"), "{}", s.status());
    assert_eq!(labels(&s), ["ref 2.00 m ±0.04"]);
    // The next line reads in metres; the live length too.
    s.keys("space k k k k k");
    assert!(s.status().contains("→ 50.0 cm"), "{}", s.status()); // under a metre: cm
    s.keys("Return");
    assert_eq!(s.mode_name(), "measure");
    assert_eq!(labels(&s)[1], "50.0 cm ±2.9");
}

#[test]
fn esc_at_the_question_stays_in_pixels_and_c_asks_again() {
    let mut s = session();
    s.keys(&format!("m {LINE} C Escape"));
    assert_eq!(
        (s.mode_name(), labels(&s)),
        ("measure", vec!["100.0 px".to_string()])
    );
    s.keys(LINE);
    s.keys("C");
    assert_eq!(s.mode_name(), "text"); // for the selected one
    s.keys("1 0 space c m Return");
    assert!(s.status().contains("1 cm = 10.0 px"), "{}", s.status());
}

#[test]
fn a_bad_length_says_why() {
    let mut s = session();
    s.keys(&format!("m {LINE} C f i v e space m Return")); // no number
    assert!(s.status().contains("can't read"), "{}", s.status());
    assert_eq!(labels(&s), ["100.0 px"]);
}

#[test]
fn circles_and_angles_finish_at_three_points() {
    let mut s = session();
    s.keys("m ^ space h h space k k space");
    assert_eq!(labels(&s)[0], "90.0°");
    s.keys("c space l l space j j space");
    assert!(labels(&s)[1].starts_with("Ø "), "{:?}", labels(&s));
}

#[test]
fn a_line_ends_at_its_second_point() {
    let mut s = session();
    s.keys("m space l l space");
    assert_eq!(s.mode_name(), "measure"); // done, in pixels
    assert_eq!(measure(&s).items[0].kind, Prim::Line);
    assert!(measure(&s).points.is_empty());
}

#[test]
fn plus_or_the_wheel_on_an_open_line_bends_it_into_an_arc() {
    let mut s = session();
    s.keys(&format!(
        "m {} plus plus plus plus plus",
        &LINE[..LINE.len() - 6]
    ));
    assert!(s.status().contains("arc 10px"), "{}", s.status());
    assert!(s.status().contains("→ 102.6 px"), "{}", s.status()); // along the arc
    s.handle(RawEvent::Wheel { steps: -2 }, 0.0);
    s.handle(RawEvent::Wheel { steps: 2 }, 0.0);
    s.keys("space");
    let m = measure(&s);
    assert_eq!(m.items[0].kind, Prim::Arc);
    // A 100px chord bulging 10px up: radius 130, about 102.65 along.
    assert_eq!(m.items[0].points[1], PixelPoint::new(709, 249));
    let v = m.items[0].values();
    assert!((v.radius.unwrap() - 130.0).abs() < 1e-9);
    assert!((v.length.unwrap() - 102.65).abs() < 0.01, "{:?}", v.length);
    // The next line starts straight; below zero bends the other way.
    s.keys("space l l l l minus minus space");
    let m = measure(&s);
    assert_eq!(m.items[1].kind, Prim::Arc);
    assert!(
        m.items[1].points[1].y > m.items[1].points[0].y,
        "below the chord"
    );
}

#[test]
fn with_no_line_open_plus_is_the_step() {
    let mut s = session();
    s.keys("m plus");
    assert_eq!(measure(&s).step, 6);
    s.keys("space plus");
    assert_eq!(measure(&s).step, 6, "an open line bends instead");
    assert_eq!(measure(&s).bend, 2.0);
}

#[test]
fn shift_keeps_a_line_going_as_a_path() {
    let mut s = session();
    s.keys("m space l l <S-space> j j <S-space> l l space");
    let m = measure(&s);
    assert_eq!((m.items[0].kind, m.items[0].points.len()), (Prim::Line, 4));
    // Enter finishes a path too, at the cursor.
    s.keys("space l l <S-space> j j Return");
    assert_eq!(measure(&s).items[1].points.len(), 3);
    // A Shift+click adds a point the same way.
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    s.button(Button::Left, p(300.5, 200.5), true);
    s.button(Button::Left, p(300.5, 200.5), false);
    s.button(Button::Left, p(320.5, 200.5), true);
    s.button(Button::Left, p(320.5, 200.5), false);
    assert_eq!(measure(&s).points.len(), 2, "still open");
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    s.button(Button::Left, p(340.5, 210.5), true);
    s.button(Button::Left, p(340.5, 210.5), false);
    assert_eq!(measure(&s).items[2].points.len(), 3);
}

#[test]
fn ctrl_click_locks_to_45_degrees() {
    let mut s = session();
    s.keys("m");
    s.button(Button::Left, p(300.5, 200.5), true);
    s.button(Button::Left, p(300.5, 200.5), false);
    s.handle(RawEvent::Modifiers { mods: Mods::CTRL }, 0.0);
    s.button(Button::Left, p(400.5, 209.5), true); // nearly flat: flat
    s.button(Button::Left, p(400.5, 209.5), false);
    assert_eq!(measure(&s).items[0].points[1], PixelPoint::new(400, 200));
}

#[test]
fn r_then_enter_measures_the_box() {
    let mut s = session();
    s.keys("m r Return");
    assert_eq!(labels(&s), ["50400 px²"]);
    let v = measure(&s).items[0].values();
    assert_eq!((v.width, v.height), (Some(560.0), Some(90.0)));
}

/// A picture with a 20x10 room round wherever you click, unless it's
/// right of x = 1000 (open); it notes the walls it was given.
struct Rooms(std::rc::Rc<std::cell::RefCell<Vec<(PixelPoint, PixelPoint)>>>);

impl EdgeFinder for Rooms {
    fn edge(&mut self, _: &Session, _: PixelPoint, _: Dir, _: PixelRect) -> Option<PixelPoint> {
        None
    }

    fn region(
        &mut self,
        _: &Session,
        seed: PixelPoint,
        _: PixelRect,
        walls: &[(PixelPoint, PixelPoint)],
    ) -> Result<Vec<PixelPoint>, String> {
        *self.0.borrow_mut() = walls.to_vec();
        if seed.x > 1000 {
            return Err("not closed in: draw lines to close it".into());
        }
        let (x, y) = (seed.x, seed.y);
        Ok(vec![
            PixelPoint::new(x - 10, y - 5),
            PixelPoint::new(x + 10, y - 5),
            PixelPoint::new(x + 10, y + 5),
            PixelPoint::new(x - 10, y + 5),
        ])
    }
}

#[test]
fn a_then_a_click_or_space_inside_measures_the_area_round_it() {
    let walls = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let mut s = session();
    s.set_edge_finder(Box::new(Rooms(walls.clone())));
    s.keys(&format!("m {LINE} A space")); // at the cursor, the line's end
    let m = measure(&s);
    assert_eq!((m.items[1].kind, m.items[1].points.len()), (Prim::Area, 4));
    assert_eq!(labels(&s)[1], "200 px²");
    assert!(m.points.is_empty(), "no polygon in progress");
    // The drawn line was a wall for it.
    assert_eq!(
        *walls.borrow(),
        [(PixelPoint::new(659, 259), PixelPoint::new(759, 259))]
    );
    click(&mut s, 500.5, 300.5);
    assert_eq!(measure(&s).items.len(), 3);
    s.keys("u");
    assert_eq!(measure(&s).items.len(), 2, "one undo step");
    click(&mut s, 1500.5, 300.5); // open: refused, saying why
    assert_eq!(measure(&s).items.len(), 2);
    assert!(s.status().contains("not closed in"), "{}", s.status());
}

#[test]
fn without_a_picture_to_flood_a_says_so() {
    let mut s = session();
    s.keys("m A space");
    assert!(measure(&s).items.is_empty());
    assert!(s.status().contains("no picture"), "{}", s.status());
}

#[test]
fn u_takes_back_a_point_then_the_calibration_then_the_line() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 1 space m Return"));
    s.keys("space l u");
    assert!(measure(&s).points.is_empty());
    assert_eq!(at(&s), (764, 259)); // the cursor stays
    s.keys("u");
    assert!(measure(&s).scale.is_none(), "the calibration, its own step");
    assert_eq!(measure(&s).items.len(), 1);
    s.keys("u");
    assert!(measure(&s).items.is_empty());
    s.keys("u");
    assert!(s.status().contains("nothing to undo"), "{}", s.status());
}

#[test]
fn points_snap_to_points_unless_alt() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
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
    s.keys("m space j j Return");
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
    s.keys(&format!("m {LINE} C 1 space m Return"));
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
    s.keys(&format!("m {LINE} {LINE} {LINE}"));
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
    s.keys(&format!("m {LINE} C 5 space m Return"));
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
fn esc_warns_before_dropping_measurements_and_the_scale_is_kept() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 1 space m Return Escape"));
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
    s.keys("m ^ space l");
    assert_eq!(measure(&s).tool, Prim::Angle);
    s.keys("s");
    assert_eq!(measure(&s).tool, Prim::Line);
    assert!(measure(&s).points.is_empty());
    // The tools are in the palette, always shown; not repeated under ?.
    assert!(s.palette()[0].contains("s line"));
    s.keys("question");
    assert!(!s.legend().join(" ").contains("s line"));
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

fn click(s: &mut Session, x: f64, y: f64) {
    s.button(Button::Left, p(x, y), true);
    s.button(Button::Left, p(x, y), false);
}

#[test]
fn a_shift_click_near_a_vertex_lands_on_it_exactly() {
    let mut s = session();
    s.keys(&format!("m {LINE}")); // 659,259 to 759,259
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    click(&mut s, 768.5, 266.5); // 9px right, 7 down: within reach
    assert_eq!(measure(&s).points, [PixelPoint::new(759, 259)]);
    s.keys("Escape");
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    click(&mut s, 768.5, 266.5); // a plain click only snaps within 4px
    assert_eq!(measure(&s).points, [PixelPoint::new(768, 266)]);
}

#[test]
fn caret_on_a_vertex_two_lines_share_measures_the_angle_between_them() {
    let mut s = session();
    s.keys(&format!("m {LINE}")); // 659,259 to 759,259
    s.keys("space k k k k k k k k k k space"); // from its end, up 50px
    assert_eq!(measure(&s).items.len(), 2);
    s.keys("^");
    click(&mut s, 761.5, 258.5); // on the shared end, near enough
    let m = measure(&s);
    assert_eq!(m.items.len(), 3, "measured at once");
    assert_eq!(m.items[2].kind, Prim::Angle);
    assert_eq!(m.items[2].points[1], PixelPoint::new(759, 259));
    assert_eq!(labels(&s)[2], "90.0°");
    assert!(m.points.is_empty());
}

#[test]
fn caret_on_a_paths_corner_measures_it_and_elsewhere_waits_for_three_points() {
    let mut s = session();
    // A path: right 50, then down-right at 45°.
    s.keys("m space l l l l l l l l l l <S-space> l l l l j j j j space");
    s.keys("^");
    click(&mut s, 709.5, 259.5); // its corner
    assert_eq!(measure(&s).items.len(), 2);
    assert_eq!(labels(&s)[1], "135.0°");
    // A vertex only one line touches: the angle is drawn as usual.
    click(&mut s, 659.5, 259.5);
    assert_eq!(measure(&s).items.len(), 2);
    assert_eq!(measure(&s).points.len(), 1);
}

#[test]
fn a_length_with_no_unit_makes_the_reference_the_unit() {
    let mut s = session();
    s.keys(&format!("m {LINE} C Return")); // Enter alone: the line is 1
    assert!(s.status().contains("1 u = 100.0 px"), "{}", s.status());
    assert!(labels(&s)[0].starts_with("ref 1.00 u"), "{:?}", labels(&s));
    s.keys("space j j j j j j j j j j space"); // 50px: half of it
    assert!(labels(&s)[1].starts_with("0.500 u"), "{:?}", labels(&s));
    s.keys("t"); // nothing to convert plain units to
    assert!(
        s.status().contains("u: nothing to convert to"),
        "{}",
        s.status()
    );
    assert!(labels(&s)[1].starts_with("0.500 u"));
    s.keys("T"); // pixels still work
    assert_eq!(labels(&s)[1], "50.0 px");
    // A number alone: the line is that many.
    let mut s = session();
    s.keys(&format!("m {LINE} C 4 Return"));
    assert!(labels(&s)[0].starts_with("ref 4.00 u"), "{:?}", labels(&s));
}

#[test]
fn space_in_the_loupe_drops_a_point_without_leaving_it() {
    let mut s = session();
    s.keys("m z"); // the loupe, at Measure's cursor
    assert_eq!(s.mode_name(), "loupe");
    s.keys("space l l l space");
    assert_eq!(s.mode_name(), "loupe", "still up after the line's done");
    s.keys("z");
    let m = measure(&s);
    assert_eq!(
        m.items[0].points,
        [PixelPoint::new(659, 259), PixelPoint::new(662, 259)]
    );
    // With a scale set, the loupe stays up: point after point.
    let mut s = session();
    s.set_measure_scale(screenie_core::units::Scale::parse("10px=1m").unwrap());
    s.keys("m z space l l <S-space> j j space");
    assert_eq!(s.mode_name(), "loupe");
    let m = s
        .modes()
        .iter()
        .find_map(|m| match m {
            Mode::Measure(m) => Some(m),
            _ => None,
        })
        .unwrap();
    assert_eq!(m.items[0].points.len(), 3, "Shift+Space kept it going");
    s.keys("z");
    assert_eq!(s.mode_name(), "measure");
    assert_eq!(at(&s), (661, 261), "Measure's cursor followed the loupe");
}

#[test]
fn holding_ctrl_locks_the_cursor_to_0_45_or_90_degrees_from_the_last_point() {
    let mut s = session();
    s.keys("m");
    click(&mut s, 300.5, 200.5);
    s.motion(p(300.5, 200.5)); // the pointer's first report
    s.handle(RawEvent::Modifiers { mods: Mods::CTRL }, 0.0);
    s.motion(p(400.5, 209.5)); // nearly flat: flat
    assert_eq!(at(&s), (400, 200));
    assert!(s.status().contains("45° lock"), "{}", s.status());
    s.motion(p(304.5, 290.5)); // nearly straight down: down
    assert_eq!(at(&s), (300, 290));
    s.motion(p(380.5, 282.5)); // near the diagonal: on it
    let (x, y) = at(&s);
    assert_eq!(x - 300, y - 200, "({x},{y})");
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    s.motion(p(380.5, 270.5)); // let go: free again
    assert_eq!(at(&s), (380, 270));
    assert!(!s.status().contains("45° lock"));
}

#[test]
fn any_unit_name_will_do_and_values_read_in_it() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 2 space l y Return"));
    assert!(s.status().contains("1 ly = 50.0 px"), "{}", s.status());
    assert!(labels(&s)[0].starts_with("ref 2.00 ly"), "{:?}", labels(&s));
    s.keys("space j j j j j j j j j j space");
    assert!(labels(&s)[1].starts_with("1.00 ly"), "{:?}", labels(&s));
}

#[test]
fn c_with_no_line_or_arc_selected_says_how() {
    let mut s = session();
    s.keys("m C");
    assert_eq!(s.mode_name(), "measure");
    assert!(
        s.status().contains("select a line or arc"),
        "{}",
        s.status()
    );
    s.keys("^ space h h space k k space C"); // an angle is selected: no length
    assert!(
        s.status().contains("select a line or arc"),
        "{}",
        s.status()
    );
}

#[test]
fn a_shift_click_near_a_line_lands_on_it_and_a_vertex_wins_near_its_ends() {
    let mut s = session();
    s.keys(&format!("m {LINE}")); // 659,259 to 759,259
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    click(&mut s, 709.5, 267.5); // 8px below its middle
    assert_eq!(measure(&s).points, [PixelPoint::new(709, 259)]);
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    s.keys("Escape");
    // Nearer the line than the end, but within reach of the end: the end.
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    click(&mut s, 752.5, 264.5);
    assert_eq!(measure(&s).points, [PixelPoint::new(759, 259)]);
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    s.keys("Escape");
    // Out of reach, nothing; and a path's segments count too.
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0);
    click(&mut s, 709.5, 280.5);
    assert_eq!(measure(&s).points, [PixelPoint::new(709, 280)]);
}

#[test]
fn the_loupe_drops_exactly_even_beside_a_line() {
    let mut s = session();
    s.keys(&format!("m {LINE}")); // 659,259 to 759,259
    s.keys("z h h h h h h h h h h j j j <S-space>"); // 3px below the line
    s.keys("z");
    assert_eq!(measure(&s).points, [PixelPoint::new(749, 262)]);
}

#[test]
fn the_status_with_an_angle_or_circle_half_drawn_doesnt_crash() {
    // The live length took the tool's own kind for two points: an angle
    // or circle needs three, and the status (drawn every frame) panicked.
    for tool in ["^", "c"] {
        let mut s = session();
        s.keys(&format!("m {tool} space l l"));
        assert!(s.status().contains("→ 10.0 px"), "{tool}: {}", s.status());
        s.keys("space j j");
        let _ = s.status();
    }
}

fn degrees_of(s: &Session) -> Vec<String> {
    measure(s)
        .items
        .iter()
        .filter(|i| i.kind == Prim::Angle)
        .map(|i| screenie_core::units::degrees(i.values().degrees.unwrap()))
        .collect()
}

fn line(s: &mut Session, a: (f64, f64), b: (f64, f64)) {
    s.handle(RawEvent::Modifiers { mods: Mods::SHIFT }, 0.0); // onto lines
    click(s, a.0, a.1);
    s.handle(
        RawEvent::Modifiers {
            mods: Mods::default(),
        },
        0.0,
    );
    click(s, b.0, b.1);
}

#[test]
fn caret_on_a_t_junction_measures_both_sides_and_not_the_straight_back() {
    let mut s = session();
    s.keys("m");
    line(&mut s, (659.5, 259.5), (759.5, 259.5)); // a line across
    line(&mut s, (709.5, 261.5), (709.5, 209.5)); // one up from its middle
    s.keys("^");
    click(&mut s, 710.5, 258.5);
    assert_eq!(degrees_of(&s), ["90.0°", "90.0°"]);
}

#[test]
fn caret_where_two_lines_cross_measures_all_four() {
    let mut s = session();
    s.keys("m");
    line(&mut s, (659.5, 259.5), (759.5, 259.5));
    line(&mut s, (689.5, 229.5), (729.5, 289.5)); // crossing it, not at an end
    s.keys("^");
    click(&mut s, 711.5, 261.5); // near the crossing (709, 259)
    let d = degrees_of(&s);
    assert_eq!(d.len(), 4, "{d:?}");
    // Opposite angles match, neighbours make 180.
    assert_eq!((d[0].clone(), d[1].clone()), (d[2].clone(), d[3].clone()));
    let a: f64 = d[0].trim_end_matches('°').parse().unwrap();
    let b: f64 = d[1].trim_end_matches('°').parse().unwrap();
    assert!((a + b - 180.0).abs() < 0.2, "{d:?}");
}

#[test]
fn caret_on_a_three_way_vertex_measures_each_gap() {
    let mut s = session();
    s.keys("m");
    line(&mut s, (709.5, 259.5), (759.5, 259.5)); // right
    line(&mut s, (709.5, 259.5), (709.5, 209.5)); // up
    line(&mut s, (709.5, 259.5), (659.5, 209.5)); // up-left
    s.keys("^");
    click(&mut s, 709.5, 259.5);
    // Right to up 90, up to up-left 45; the rest round (225) is the outside.
    assert_eq!(degrees_of(&s), ["90.0°", "45.0°"]);
}

#[test]
fn the_palette_shows_the_primitives_always() {
    let mut s = session();
    s.keys("m");
    assert_eq!(
        s.palette(),
        ["s line  c circle  A area  r rect  ^ angle"],
        "without ?"
    );
}

fn names(s: &Session) -> Vec<String> {
    measure(s).items.iter().map(|i| i.name.clone()).collect()
}

#[test]
fn u_after_a_delete_brings_it_back_where_it_was() {
    let mut s = session();
    s.keys(&format!("m {LINE} {LINE} {LINE}"));
    s.keys("bracketright d"); // the middle one
    assert_eq!(names(&s), ["L1", "L3"]);
    s.keys("u");
    assert_eq!(names(&s), ["L1", "L2", "L3"]);
    s.keys("u"); // then the last added
    assert_eq!(names(&s), ["L1", "L2"]);
}

#[test]
fn u_takes_back_a_calibration_on_its_own() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 1 space m Return"));
    assert!(measure(&s).scale.is_some());
    s.keys("u");
    assert!(measure(&s).scale.is_none() && measure(&s).reference.is_none());
    assert_eq!(names(&s), ["L1"], "the line stays");
    assert_eq!(labels(&s), ["100.0 px"]);
    // Recalibrating, u goes back to the old scale, not none.
    s.keys("C 1 space m Return C 2 space m Return u");
    assert!(s.status().contains("1 m = 100.0 px"), "{}", s.status());
}

#[test]
fn deleting_the_reference_and_undoing_restores_it() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 1 space m Return d"));
    assert!(measure(&s).reference.is_none());
    s.keys("u");
    assert_eq!(measure(&s).reference.as_deref(), Some("L1"));
    assert!(labels(&s)[0].starts_with("ref"));
}

#[test]
fn u_takes_back_x_and_a_reorder_and_a_vertexs_angles_at_once() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
    s.keys("space k k k k k k k k k k space"); // up from its end
    s.keys("x");
    assert!(measure(&s).items[1].excluded);
    s.keys("u");
    assert!(!measure(&s).items[1].excluded);
    s.keys("greater"); // L2 up the list
    assert_eq!(names(&s), ["L2", "L1"]);
    assert_eq!(measure(&s).selected, Some(0), "selection follows it");
    s.keys("u");
    assert_eq!(names(&s), ["L1", "L2"]);
    s.keys("^");
    click(&mut s, 759.5, 259.5); // the shared end: one angle
    assert_eq!(names(&s), ["L1", "L2", "∠1"]);
    s.keys("u");
    assert_eq!(names(&s), ["L1", "L2"]);
}

#[test]
fn a_short_reference_is_warned_about_while_it_lasts() {
    let mut s = session();
    // 50px: about ±2.8%, over the 2% a reference should be good to.
    s.keys("m space l l l l l l l l l l space C 1 space m Return");
    assert!(s.status().contains("short reference"), "{}", s.status());
    s.keys("l l j"); // still, after other keys
    assert!(s.status().contains("short reference"), "{}", s.status());
    // A long one isn't.
    let mut s = session();
    s.keys("m space 0 space C 1 space m Return"); // to the monitor's left edge
    assert!(!s.status().contains("short reference"), "{}", s.status());
}

#[test]
fn the_scale_travels_in_the_project_so_edit_last_keeps_it() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 4 space l y Return"));
    let json = s.project().to_json();
    assert!(json.contains("\"measure_scale\""), "{json}");
    let mut again = session();
    again.restore(screenie_core::model::Project::from_json(&json).unwrap());
    again.keys("m");
    assert!(
        again.status().contains("1 ly = 25.0 px"),
        "{}",
        again.status()
    );
    // A project without one reads as before, and writes nothing for it.
    let plain = session().project().to_json();
    assert!(!plain.contains("measure_scale"), "{plain}");
}

fn alt(s: &mut Session, on: bool) {
    let mods = if on { Mods::ALT } else { Mods::default() };
    s.handle(RawEvent::Modifiers { mods }, 0.0);
}

#[test]
fn alt_click_selects_a_measurement_and_places_nothing() {
    let mut s = session();
    s.keys(&format!("m {LINE}")); // L1: 659,259 to 759,259
    s.keys("space j j j j j j j j j j space"); // L2 down from its end
    assert_eq!(measure(&s).selected, Some(1));
    alt(&mut s, true);
    click(&mut s, 700.5, 265.5); // 6px off L1's middle
    alt(&mut s, false);
    assert_eq!(measure(&s).selected, Some(0));
    assert!(measure(&s).points.is_empty(), "no point placed");
    alt(&mut s, true);
    click(&mut s, 1200.5, 700.5); // nowhere near one
    alt(&mut s, false);
    assert_eq!(measure(&s).selected, Some(0), "kept");
}

#[test]
fn alt_drag_moves_a_point_and_u_puts_it_back() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
    alt(&mut s, true);
    s.button(Button::Left, p(758.5, 260.5), true); // on L1's end
    s.motion(p(758.5, 260.5));
    s.motion(p(770.5, 280.5));
    s.button(Button::Left, p(770.5, 280.5), false);
    alt(&mut s, false);
    assert_eq!(
        measure(&s).items[0].points,
        [PixelPoint::new(659, 259), PixelPoint::new(770, 280)]
    );
    s.keys("u");
    assert_eq!(measure(&s).items[0].points[1], PixelPoint::new(759, 259));
}

#[test]
fn o_edits_the_selected_measurements_points_from_the_keys() {
    let mut s = session();
    s.keys(&format!("m {LINE}"));
    s.keys("o"); // its first point
    assert_eq!(at(&s), (659, 259));
    assert!(
        s.status().contains("editing L1 point 1/2"),
        "{}",
        s.status()
    );
    s.keys("k k"); // the point follows the cursor
    assert_eq!(measure(&s).items[0].points[0], PixelPoint::new(659, 249));
    s.keys("o j j"); // the next point
    assert_eq!(measure(&s).items[0].points[1], PixelPoint::new(759, 269));
    s.keys("Return");
    assert!(!s.status().contains("editing"));
    s.keys("u"); // the whole edit, one step
    assert_eq!(
        measure(&s).items[0].points,
        [PixelPoint::new(659, 259), PixelPoint::new(759, 259)]
    );
    // Esc puts it back as it was.
    s.keys("o l l l Escape");
    assert_eq!(measure(&s).items[0].points[0], PixelPoint::new(659, 259));
    assert_eq!(s.mode_name(), "measure");
}

#[test]
fn editing_the_reference_rescales_to_the_same_real_length() {
    let mut s = session();
    s.keys(&format!("m {LINE} C 1 space m Return")); // 100px is 1 m
    s.keys("o o l l l l l l l l l l l l l l l l l l l l Return"); // its end, +100px
    assert!(s.status().contains("1 m = 200.0 px"), "{}", s.status());
    s.keys("u");
    assert!(s.status().contains("1 m = 100.0 px"), "{}", s.status());
}

/// A scale bar under LINE (659..759 at y 259): "0" and "5" at its ends, "m".
fn bar() -> screenie_core::ocr::OcrResult {
    use screenie_core::ocr::{OcrResult, OcrWord};
    let word = |t: &str, x| OcrWord {
        text: t.into(),
        bbox: PixelRect::new(x, 266, 8 * t.len() as i32, 12),
        confidence: Some(90.0),
        line: 0,
    };
    OcrResult {
        text: "0 5 m".into(),
        words: vec![word("0", 655), word("5", 755), word("m", 775)],
        codes: vec![],
    }
}

struct ReadsNow;

impl screenie_core::session::Analyzer for ReadsNow {
    fn start(
        &mut self,
        _: &Session,
        _: u64,
        _: screenie_core::session::JobKind,
    ) -> Option<screenie_core::session::JobResult> {
        Some(screenie_core::session::JobResult::Ocr(Ok(bar())))
    }
}

struct ReadsLater(std::rc::Rc<std::cell::RefCell<Vec<u64>>>);

impl screenie_core::session::Analyzer for ReadsLater {
    fn start(
        &mut self,
        _: &Session,
        id: u64,
        _: screenie_core::session::JobKind,
    ) -> Option<screenie_core::session::JobResult> {
        self.0.borrow_mut().push(id);
        None
    }
}

#[test]
fn c_reads_the_scale_bar_the_line_runs_along() {
    let mut s = session();
    s.set_analyzer(Box::new(ReadsNow));
    s.keys(&format!("m {LINE} C"));
    assert!(
        s.status().contains("= 5 m|  read off the scale bar"),
        "{}",
        s.status()
    );
    s.keys("Return");
    assert!(s.status().contains("1 m = 20.0 px"), "{}", s.status());
    // Typing replaces a reading.
    s.keys("C 2 space m Return");
    assert!(s.status().contains("1 m = 50.0 px"), "{}", s.status());
}

#[test]
fn a_late_reading_fills_the_prompt_unless_typing_started() {
    let jobs = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let mut s = session();
    s.set_analyzer(Box::new(ReadsLater(jobs.clone())));
    s.keys(&format!("m {LINE} C"));
    assert!(
        s.status().contains("reading the scale bar"),
        "{}",
        s.status()
    );
    assert!(s.wants_ticks(), "the host keeps polling for it");
    let id = jobs.borrow()[0];
    s.job_done(id, screenie_core::session::JobResult::Ocr(Ok(bar())));
    assert!(s.status().contains("= 5 m|"), "{}", s.status());
    s.keys("Escape C 3"); // typing first: a late reading doesn't overwrite it
    let id = jobs.borrow()[1];
    s.job_done(id, screenie_core::session::JobResult::Ocr(Ok(bar())));
    assert!(s.status().contains("= 3|"), "{}", s.status());
}
