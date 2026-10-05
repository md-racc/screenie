//! Annotate (the pen, inks, width) and its Shape child. Ported from the
//! prototype's tests/test_modes.py and tests/test_audit.py.

mod common;

use common::*;
use screenie_core::annotate::ShapeTool;
use screenie_core::geometry::PixelPoint;
use screenie_core::keys::Mods;
use screenie_core::model::OpData;
use screenie_core::session::{Button, Mode, Outcome, Pending, RawEvent, Sampler, Session};

fn kinds(s: &Session) -> Vec<String> {
    s.open_layer()
        .map(|l| {
            l.ops
                .iter()
                .map(|o| match &o.data {
                    OpData::Censor(_) => "censor".to_string(),
                    OpData::Stroke(_) => "stroke".to_string(),
                    OpData::Shape(sh) => sh.kind.name().to_string(),
                    OpData::Filter(_) => "filter".to_string(),
                    OpData::Line(l) => l.heads_name().to_string(),
                    OpData::Highlight(_) => "highlight".to_string(),
                    other => other.noun().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ink(s: &Session) -> (String, f64) {
    let (i, w) = s.ink();
    (i.name.clone(), w)
}

/// A picture that's light grey everywhere (the prototype fixture's text).
struct Grey;

impl Sampler for Grey {
    fn sample(&mut self, _: &Session, _: PixelPoint) -> Option<[u8; 3]> {
        Some([217, 217, 216])
    }
}

#[test]
fn the_pen_draws_into_the_layer_and_enter_commits() {
    let mut s = session();
    s.keys("a");
    assert_eq!(s.mode_name(), "annotate");
    s.keys("4 plus");
    assert_eq!(ink(&s), ("green".into(), 4.0));
    s.keys("drag:50,150:300,200"); // starts outside the box, ends inside it
    assert_eq!(kinds(&s), ["stroke"]); // locked into the layer only
    assert!(s.layers().is_empty());
    s.keys("click:200,240"); // a dot
    assert_eq!(kinds(&s), ["stroke", "stroke"]);
    s.keys("u"); // drops the dot
    assert_eq!(kinds(&s), ["stroke"]);
    s.keys("Return");
    assert_eq!(s.mode_name(), "select");
    let layer = &s.layers()[0];
    assert_eq!(layer.clip, START); // clipped to the box when drawn
    let OpData::Stroke(st) = &layer.ops[0].data else {
        panic!()
    };
    assert_eq!(
        (st.points.first(), st.points.last()),
        (Some(&p(50.0, 150.0)), Some(&p(300.0, 200.0)))
    );
    assert_eq!((st.rgb, st.width), ([0.1, 0.75, 0.25], 4.0));
    s.keys("u");
    assert!(s.layers().is_empty());
}

#[test]
fn inks_and_the_eyedropper() {
    let mut s = session();
    s.set_sampler(Box::new(Grey));
    s.keys("a");
    let mut names = vec![];
    for k in ["1", "2", "3", "4", "5"] {
        s.keys(k);
        names.push(ink(&s).0);
    }
    assert_eq!(names, ["white", "black", "red", "green", "blue"]);
    s.motion(p(125.5, 196.5)); // over the text's light grey
    s.keys("p");
    assert_eq!(ink(&s).0, "#d9d9d8");
    assert_eq!(s.ink().0.rgb, [217.0 / 255.0, 217.0 / 255.0, 216.0 / 255.0]);
    s.keys("3"); // a number key replaces it
    assert_eq!(ink(&s).0, "red");
}

#[test]
fn a_locks_the_shape_and_goes_back_for_another() {
    let mut s = session();
    s.keys("a a"); // the pen: a does nothing
    assert_eq!(s.mode_name(), "annotate");
    s.keys("r l j a"); // grow a rect from the point, lock it, back to the pen
    assert_eq!(s.mode_name(), "annotate");
    s.keys("c l j a"); // a circle too
    assert_eq!(kinds(&s), ["rect", "circle"]);
    assert!(s.layers().is_empty()); // nothing committed yet
    s.keys("c u"); // in a shape: u drops the last locked one
    assert_eq!(kinds(&s), ["rect"]);
    s.keys("Escape Return"); // drop the shape, commit from the pen
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers().len(), 1);
}

#[test]
fn a_shape_starts_as_a_point_at_the_pointer_and_edits_like_a_box() {
    let mut s = session();
    s.keys("a r"); // the pen's pointer starts at the box's centre
    let Some(Mode::Shape(sh)) = s.modes().last() else {
        panic!()
    };
    assert_eq!((sh.kind, sh.sel.bounds()), (ShapeTool::Rect, START));
    assert_eq!(sh.sel.rect(), r(380.0, 215.0, 0.0, 0.0));
    s.keys("l t");
    let Some(Mode::Shape(sh)) = s.modes().last() else {
        panic!()
    };
    assert_eq!((sh.kind, sh.sel.rect().w), (ShapeTool::Circle, 5.0));
    s.keys("Escape"); // drop just this shape
    assert_eq!(s.mode_name(), "annotate");
    assert!(kinds(&s).is_empty());
    s.keys("r l j Return"); // lock it and commit the whole layer
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers()[0].ops.len(), 1);
}

#[test]
fn escape_in_a_shape_only_drops_the_shape() {
    let mut s = session();
    s.keys("a r l j a c Escape");
    assert_eq!((s.mode_name(), kinds(&s).len()), ("annotate", 1));
    assert_eq!(s.pending(), None);
}

#[test]
fn holding_plus_keeps_widening_the_line_and_other_keys_fire_once() {
    let mut s = session();
    s.handle(
        RawEvent::RepeatInfo {
            rate: 10.0,
            delay: 0.5,
        },
        0.0,
    );
    s.keys("a");
    s.press(9, "plus", Mods::NONE, 0.0);
    s.tick(0.95); // the press, and five repeats by then
    s.release(9, "plus", Mods::NONE, 0.95);
    assert_eq!(ink(&s).1, 3.0 + 6.0);
    s.keys("c minus"); // in a shape box too
    assert_eq!(ink(&s).1, 3.0 + 6.0 - 1.0);
    s.press(10, "t", Mods::NONE, 2.0);
    s.press(10, "t", Mods::NONE, 2.1); // the platform's autorepeat
    s.tick(4.0);
    let Some(Mode::Shape(sh)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(sh.kind, ShapeTool::Line); // circle, then the next once
}

#[test]
fn width_stops_at_its_range() {
    let mut s = session();
    s.keys(&format!("a {}", "plus ".repeat(50)));
    assert_eq!(ink(&s).1, 40.0);
    s.keys(&"minus ".repeat(50));
    assert_eq!(ink(&s).1, 1.0);
}

#[test]
fn the_status_line_shows_the_ink_and_no_cursor_for_the_pen() {
    let mut s = session();
    s.keys("a");
    assert_eq!(s.status(), "ANNOTATE  560×90  red 3px  layer");
    s.keys("Escape");
    assert!(s.status().starts_with("SELECT"));
    s.keys("a r l l j");
    assert_eq!(
        s.status(),
        "SHAPE  560×90  (389,219)  red 3px  layer  rect  10×5"
    );
}

#[test]
fn the_palettes_show_what_the_keys_pick() {
    let mut s = session();
    // Select's: the modes it opens, and what it finishes with.
    assert_eq!(
        s.palette(),
        [
            "i insert  z loupe  e enhance  x censor  a annotate  f filter  m measure  1 layers",
            "Enter image  y text  p colour  Q code  P pin"
        ]
    );
    s.keys("x");
    assert_eq!(s.palette(), ["t pixelate/bar  r find secrets"]);
    s.keys("Escape");
    s.keys("a");
    assert_eq!(
        s.palette(),
        ["1 white  2 black  3 red  4 green  5 blue  r rectangle  c circle  v arrow"]
    );
    s.keys("r");
    assert_eq!(
        s.palette(),
        ["1 white  2 black  3 red  4 green  5 blue  t rect/circle/line/arrow"]
    );
    assert!(!s.legend().join(" ").contains("5 blue")); // the legend is the rest
}

#[test]
fn enter_mid_stroke_keeps_the_line() {
    let mut s = session();
    s.keys("a");
    s.button(Button::Left, p(200.0, 200.0), true);
    s.motion(p(300.0, 220.0));
    s.button(Button::Right, p(300.0, 220.0), true); // right click is Enter
    s.button(Button::Left, p(300.0, 220.0), false);
    assert_eq!(s.layers().len(), 1);
    assert_eq!(s.layers()[0].ops.len(), 1);
}

#[test]
fn new_work_takes_the_discard_warning_back() {
    let mut s = session();
    s.keys("a drag:200,200:250,220 Escape");
    assert_eq!(s.pending(), Some(Pending::Discard));
    s.button(Button::Left, p(300.0, 200.0), true); // another line
    s.motion(p(350.0, 220.0));
    s.button(Button::Left, p(350.0, 220.0), false);
    assert_eq!(s.pending(), None);
    s.button(Button::Middle, p(0.0, 0.0), true); // Esc: only warns again
    assert_eq!((s.mode_name(), kinds(&s).len()), ("annotate", 2));
}

#[test]
fn tools_share_one_layer_and_undo_crosses_them() {
    let mut s = session();
    s.keys("a 5 drag:370,205:390,225");
    s.keys("x"); // straight to Censor, same layer
    assert_eq!(s.mode_name(), "censor");
    s.keys("t x"); // a bar locked
    s.keys("a"); // back to the pen: the censor being edited is locked too
    assert_eq!(s.mode_name(), "annotate");
    assert_eq!(kinds(&s), ["stroke", "censor", "censor"]);
    s.keys("u"); // the last op, whichever tool made it
    assert_eq!(kinds(&s), ["stroke", "censor"]);
    s.keys("Return");
    assert_eq!((s.mode_name(), s.layers().len()), ("select", 1));
    assert_eq!(s.layers()[0].ops.len(), 2);
    s.keys("u");
    assert!(s.layers().is_empty());
}

#[test]
fn switching_tools_returns_to_where_the_layer_opened() {
    let mut s = session();
    s.keys("a r l j x"); // from a shape straight to Censor: Shape and the pen go
    assert_eq!(
        s.modes().iter().map(Mode::name).collect::<Vec<_>>(),
        ["censor"]
    );
    assert_eq!(kinds(&s), ["rect"]);
}

#[test]
fn q_quits_from_the_pen_and_a_shape() {
    for keys in ["a q", "a r q"] {
        let mut s = session();
        s.keys(keys);
        assert_eq!(s.outcome(), Some(&Outcome::Cancelled), "{keys}");
    }
}

#[test]
fn ctrl_and_alt_hjkl_mean_nothing_to_the_pen_and_the_box_stays() {
    let mut s = session();
    s.keys("a <C-j> <A-k>");
    assert_eq!((s.mode_name(), s.rect()), ("annotate", START));
}

#[test]
fn lines_and_arrows_run_anchor_to_cursor_and_o_flips_them() {
    use screenie_core::model::Heads;
    let mut s = session();
    // An arrow starts as a point at the pointer (the box's centre): the
    // tail stays there and the tip, the cursor, grows it.
    s.keys("a v l l l l j j a");
    let OpData::Line(l) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    assert_eq!(
        (l.a, l.b, l.heads),
        (p(380.0, 215.0), p(400.0, 225.0), Heads::End)
    );
    s.keys("v l l j j o a"); // o: the cursor at the other end, so the arrow flips
    let OpData::Line(l) = &s.open_layer().unwrap().ops[1].data else {
        panic!()
    };
    assert_eq!((l.a, l.b), (p(390.0, 225.0), p(380.0, 215.0)));
    s.keys("v V l j a r t t l j a"); // a plain line: t from a rect, twice
    let ops = &s.open_layer().unwrap().ops;
    assert!(matches!(&ops[2].data, OpData::Line(l) if l.heads == Heads::Both));
    assert!(matches!(&ops[3].data, OpData::Line(l) if l.heads == Heads::None));
    assert_eq!(
        s.open_layer().map(|l| l.describe()).unwrap(),
        "arrow ×3, line"
    );
}

#[test]
fn m_toggles_the_highlighter_a_wider_translucent_pen() {
    let mut s = session();
    s.keys("a m");
    assert!(
        s.status().contains("  red 3px  highlighter"),
        "{}",
        s.status()
    );
    s.keys("drag:150,200:300,220 m drag:150,230:300,240");
    assert_eq!(kinds(&s), ["highlight", "stroke"]);
    let OpData::Highlight(h) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    assert_eq!((h.width, h.alpha), (15.0, 0.4));
}

fn texts(s: &Session) -> Vec<screenie_core::model::TextOp> {
    s.open_layer()
        .map(|l| {
            l.ops
                .iter()
                .filter_map(|o| match &o.data {
                    OpData::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn i_types_text_at_the_pen_and_letters_are_literal() {
    let mut s = session();
    s.keys("a");
    s.motion(p(200.0, 180.0));
    s.keys("i");
    assert_eq!(s.mode_name(), "text");
    s.keys("H i space q ? <S-Return> x BackSpace y"); // q and ? are text here
    assert!(s.outcome().is_none() && !s.show_help());
    s.keys("<C-l> <C-S-j> <C-b> <C-plus> <C-4>");
    s.keys("Return");
    assert_eq!(s.mode_name(), "annotate");
    let t = &texts(&s)[0];
    assert_eq!(t.text, "Hi q?\ny");
    assert_eq!((t.origin, t.size, t.chip), (p(201.0, 190.0), 20.0, false));
    assert_eq!(t.rgb, [0.1, 0.75, 0.25]); // Ctrl+4: green
}

#[test]
fn esc_drops_the_text_and_empty_text_locks_nothing() {
    let mut s = session();
    s.keys("a i x y Escape i Return");
    assert_eq!(s.mode_name(), "annotate");
    assert!(texts(&s).is_empty());
}

#[test]
fn callouts_count_up_undo_reuses_the_number_and_capital_n_sets_it() {
    let n = |s: &Session| -> Vec<u32> {
        s.open_layer()
            .unwrap()
            .ops
            .iter()
            .filter_map(|o| match &o.data {
                OpData::Callout(c) => Some(c.n),
                _ => None,
            })
            .collect()
    };
    let mut s = session();
    s.keys("a n n n u n");
    assert_eq!(n(&s), [1, 2, 3]);
    s.keys("N 7 Return n n");
    assert_eq!(n(&s), [1, 2, 3, 7, 8]);
    s.keys("N x Return");
    assert_eq!(s.note(), Some("not a number: \"x\""));
}

#[test]
fn the_keyboard_pen_moves_with_hjkl_and_space_draws() {
    let mut s = session();
    s.keys("a"); // the pen point starts at the box's centre (380, 215)
    assert_eq!(s.pen_point(), None); // not shown until the keys move it
    s.keys("h h");
    assert_eq!(
        s.pen_point(),
        Some(screenie_core::geometry::PixelPoint::new(370, 215))
    );
    s.keys("space l l L");
    assert!(s.status().contains("  red 3px  pen down"), "{}", s.status());
    s.keys("space"); // lifted: the line is locked
    let OpData::Stroke(st) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    assert_eq!(
        st.points,
        [
            p(370.0, 215.0),
            p(375.0, 215.0),
            p(380.0, 215.0),
            p(400.0, 215.0)
        ]
    );
    s.keys(&"K ".repeat(20)); // stops at the box
    assert_eq!(
        s.pen_point(),
        Some(screenie_core::geometry::PixelPoint::new(400, 170))
    );
}

#[test]
fn a_trace_keeps_one_point_a_half_pixel_however_fast_the_mouse() {
    let mut s = session();
    s.keys("a");
    s.button(Button::Left, p(200.0, 200.0), true);
    // A 1000 Hz mouse moving slowly: 0.1 px apart.
    for i in 1..=100 {
        s.motion(p(200.0 + f64::from(i) * 0.1, 200.0));
    }
    s.button(Button::Left, p(210.05, 200.0), false);
    let OpData::Stroke(st) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    // 10 px: about a point a half pixel, and the end exactly where let go.
    assert!((20..=22).contains(&st.points.len()), "{}", st.points.len());
    assert_eq!(st.points.last(), Some(&p(210.05, 200.0)));
}

fn shape_rect(s: &Session) -> screenie_core::geometry::LogicalRect {
    match s.modes().last() {
        Some(Mode::Shape(sh)) => sh.sel.rect(),
        _ => panic!("not in a shape: {}", s.mode_name()),
    }
}

#[test]
fn a_shape_starts_at_the_loupe_spot_and_its_cursor_follows_the_mouse() {
    let mut s = session();
    s.keys("z");
    s.button(Button::Left, p(300.5, 200.5), true);
    s.button(Button::Left, p(300.5, 200.5), false);
    s.keys("z a r");
    let (gx, gy) = (300.5, 200.5); // the pixel's centre
    assert_eq!(shape_rect(&s), r(gx, gy, 0.0, 0.0)); // a point
    s.motion(p(400.0, 230.0)); // the cursor corner follows; the anchor stays
    assert_eq!(shape_rect(&s), r(gx, gy, 400.0 - gx, 230.0 - gy));
    s.motion(p(250.0, 180.0)); // across the anchor, it just flips
    assert_eq!(shape_rect(&s), r(250.0, 180.0, gx - 250.0, gy - 180.0));
    s.keys("o"); // now the other corner is the cursor
    s.motion(p(320.0, 210.0));
    assert_eq!(shape_rect(&s), r(250.0, 180.0, 70.0, 30.0));
    s.keys("a");
    assert_eq!(kinds(&s), ["rect"]);
}

#[test]
fn an_arrow_from_the_loupe_spot_pins_its_tail_and_the_tip_follows() {
    use screenie_core::model::Heads;
    let mut s = session();
    s.keys("z");
    s.button(Button::Left, p(300.5, 200.5), true);
    s.button(Button::Left, p(300.5, 200.5), false);
    s.keys("z a v");
    s.motion(p(420.0, 240.0));
    s.keys("a");
    let OpData::Line(l) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    assert_eq!(
        (l.a, l.b, l.heads),
        (p(300.5, 200.5), p(420.0, 240.0), Heads::End)
    );
}

#[test]
fn without_the_loupe_a_shape_starts_at_the_pointer() {
    let mut s = session();
    s.keys("a");
    s.motion(p(500.0, 220.0));
    s.keys("c");
    assert_eq!(shape_rect(&s), r(500.0, 220.0, 0.0, 0.0));
    s.keys("l"); // hjkl grow it from there
    assert_eq!(shape_rect(&s), r(500.0, 220.0, 5.0, 0.0));
}

fn mods(s: &mut Session, m: Mods) {
    s.handle(RawEvent::Modifiers { mods: m }, 0.0);
}

fn last_points(s: &Session) -> Vec<screenie_core::geometry::LogicalPoint> {
    match &s.open_layer().unwrap().ops.last().unwrap().data {
        OpData::Stroke(st) => st.points.clone(),
        OpData::Highlight(h) => h.points.clone(),
        other => panic!("not a pen line: {other:?}"),
    }
}

/// The furthest a line's points get from the chord a→b (signed: positive
/// is to the left of a→b on screen, i.e. up when it runs rightwards).
fn bulge(pts: &[screenie_core::geometry::LogicalPoint]) -> f64 {
    let (a, b) = (pts[0], *pts.last().unwrap());
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    pts.iter()
        .map(|q| ((q.x - a.x) * dy - (q.y - a.y) * dx) / len)
        .fold(0.0, |m: f64, d| if d.abs() > m.abs() { d } else { m })
}

#[test]
fn shift_while_drawing_makes_the_line_straight() {
    let mut s = session();
    s.keys("a");
    mods(&mut s, Mods::SHIFT);
    s.button(Button::Left, p(150.0, 200.0), true);
    for q in [p(160.0, 190.0), p(170.0, 230.0), p(200.0, 200.0)] {
        s.motion(q);
    }
    s.button(Button::Left, p(250.0, 220.0), false);
    assert_eq!(last_points(&s), [p(150.0, 200.0), p(250.0, 220.0)]);
}

#[test]
fn letting_go_of_shift_carries_on_freehand_and_shift_again_starts_straight_there() {
    let mut s = session();
    s.keys("a");
    s.button(Button::Left, p(150.0, 200.0), true);
    s.motion(p(160.0, 201.0)); // freehand
    mods(&mut s, Mods::SHIFT);
    s.motion(p(170.0, 230.0));
    s.motion(p(200.0, 210.0)); // straight from 160,201
    mods(&mut s, Mods::default());
    s.motion(p(210.0, 215.0)); // freehand again, from 200,210
    mods(&mut s, Mods::SHIFT);
    s.motion(p(230.0, 240.0)); // a new straight piece, from 210,215
    s.button(Button::Left, p(240.0, 225.0), false);
    assert_eq!(
        last_points(&s),
        [
            p(150.0, 200.0),
            p(160.0, 201.0),
            p(200.0, 210.0),
            p(210.0, 215.0),
            p(240.0, 225.0)
        ]
    );
}

#[test]
fn ctrl_while_drawing_makes_an_arc_over_the_chord() {
    let mut s = session();
    s.keys("a");
    mods(&mut s, Mods::CTRL);
    s.button(Button::Left, p(200.0, 230.0), true);
    s.motion(p(300.0, 230.0));
    assert!(s.status().contains("arc 24px"), "{}", s.status());
    s.button(Button::Left, p(300.0, 230.0), false);
    let pts = last_points(&s);
    assert_eq!(
        (pts[0], *pts.last().unwrap()),
        (p(200.0, 230.0), p(300.0, 230.0))
    );
    assert!(pts.len() > 8, "a curve, not a chord: {}", pts.len());
    assert!((bulge(&pts) - 24.0).abs() < 0.5, "{}", bulge(&pts)); // up, 24px
}

#[test]
fn ctrl_plus_minus_and_ctrl_wheel_set_the_bulge_and_below_zero_flips_it() {
    let mut s = session();
    s.keys("a");
    let width = ink(&s).1;
    mods(&mut s, Mods::CTRL);
    s.button(Button::Left, p(200.0, 230.0), true);
    s.motion(p(300.0, 230.0));
    s.handle(RawEvent::Wheel { steps: 3 }, 0.0); // 2px a notch
    assert!(s.status().contains("arc 30px"), "{}", s.status());
    s.keys("<C-minus>");
    assert!(s.status().contains("arc 28px"), "{}", s.status());
    mods(&mut s, Mods::CTRL); // still held (the script let go of it)
    s.handle(RawEvent::Wheel { steps: -26 }, 0.0); // through zero: the other side
    assert!(s.status().contains("arc -24px"), "{}", s.status());
    assert_eq!(ink(&s).1, width, "the width is untouched");
    s.button(Button::Left, p(300.0, 230.0), false);
    assert!((bulge(&last_points(&s)) + 24.0).abs() < 0.5); // down now
    // The bulge is kept for the next arc; plain wheel is still the width.
    mods(&mut s, Mods::default());
    s.handle(RawEvent::Wheel { steps: 1 }, 0.0);
    assert_eq!(ink(&s).1, width + 1.0);
}

#[test]
fn an_arc_at_zero_bulge_is_straight_and_the_highlighter_bends_too() {
    let mut s = session();
    s.keys("a m");
    mods(&mut s, Mods::CTRL);
    s.button(Button::Left, p(200.0, 230.0), true);
    s.motion(p(300.0, 230.0));
    s.handle(RawEvent::Wheel { steps: -12 }, 0.0);
    s.button(Button::Left, p(300.0, 230.0), false);
    assert_eq!(last_points(&s), [p(200.0, 230.0), p(300.0, 230.0)]);
    assert!(matches!(
        s.open_layer().unwrap().ops[0].data,
        OpData::Highlight(_)
    ));
}

#[test]
fn s_no_longer_opens_a_line_shift_draws_them() {
    let mut s = session();
    s.keys("a s");
    assert_eq!(s.mode_name(), "annotate");
}
