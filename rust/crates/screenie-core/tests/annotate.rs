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
    s.keys("r a"); // lock the rect, back to the pen
    assert_eq!(s.mode_name(), "annotate");
    s.keys("c a"); // a circle too
    assert_eq!(kinds(&s), ["rect", "circle"]);
    assert!(s.layers().is_empty()); // nothing committed yet
    s.keys("c u"); // in a shape: u drops the last locked one
    assert_eq!(kinds(&s), ["rect"]);
    s.keys("Escape Return"); // drop the shape, commit from the pen
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers().len(), 1);
}

#[test]
fn a_shape_starts_in_the_middle_quarter_and_edits_like_a_box() {
    let mut s = session();
    s.keys("a r");
    let Some(Mode::Shape(sh)) = s.modes().last() else {
        panic!()
    };
    assert_eq!((sh.kind, sh.sel.bounds()), (ShapeTool::Rect, START));
    assert_eq!(sh.sel.rect(), r(240.0, 192.5, 280.0, 45.0));
    s.keys("l t");
    let Some(Mode::Shape(sh)) = s.modes().last() else {
        panic!()
    };
    assert_eq!((sh.kind, sh.sel.rect().w), (ShapeTool::Circle, 285.0));
    s.keys("Escape"); // drop just this shape
    assert_eq!(s.mode_name(), "annotate");
    assert!(kinds(&s).is_empty());
    s.keys("r Return"); // lock it and commit the whole layer
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers()[0].ops.len(), 1);
}

#[test]
fn escape_in_a_shape_only_drops_the_shape() {
    let mut s = session();
    s.keys("a r a c Escape");
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
    s.keys("a r");
    assert_eq!(
        s.status(),
        "SHAPE  560×90  (519,237)  red 3px  layer  rect  280×45"
    );
}

#[test]
fn the_palettes_show_what_the_keys_pick() {
    let mut s = session();
    assert!(s.palette().is_empty());
    s.keys("a");
    assert_eq!(
        s.palette(),
        ["1 white  2 black  3 red  4 green  5 blue  r rectangle  c circle  s straight  v arrow"]
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
    s.keys("a r x"); // from a shape straight to Censor: Shape and the pen go
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
    s.keys("a v"); // an arrow box in the middle quarter: 240,192.5 to 520,237.5
    s.keys("a");
    let OpData::Line(l) = &s.open_layer().unwrap().ops[0].data else {
        panic!()
    };
    assert_eq!(
        (l.a, l.b, l.heads),
        (p(240.0, 192.5), p(520.0, 237.5), Heads::End)
    );
    s.keys("v o a"); // o: the cursor at the other end, so the arrow flips
    let OpData::Line(l) = &s.open_layer().unwrap().ops[1].data else {
        panic!()
    };
    assert_eq!((l.a, l.b), (p(520.0, 237.5), p(240.0, 192.5)));
    s.keys("v V a s a");
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
