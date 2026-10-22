//! Insert, Loupe and Enhance: the point cursor, the lenses, view filters,
//! and positions handed back.

mod common;

use common::*;
use screenie_core::geometry::Dir;
use screenie_core::geometry::{LogicalPoint, PixelPoint};
use screenie_core::insert::InsertMode;
use screenie_core::intent::DirSet;
use screenie_core::keys::Mods;
use screenie_core::lens::ViewFilter;
use screenie_core::session::{Button, FinishedAction, Mode, Outcome, Session};

fn point(s: &Session) -> PixelPoint {
    match s.modes().last() {
        Some(Mode::Insert(m)) => m.cursor.at,
        Some(Mode::Loupe(m)) => m.cursor.at,
        _ => panic!("no point cursor in {}", s.mode_name()),
    }
}

fn insert_at(s: &Session) -> PixelPoint {
    s.modes()
        .iter()
        .find_map(|m| match m {
            Mode::Insert(i) => Some(i.cursor.at),
            _ => None,
        })
        .expect("insert on the stack")
}

fn lens(s: &Session) -> &screenie_core::lens::EnhanceMode {
    match s.modes().last() {
        Some(Mode::Enhance(e)) => e,
        _ => panic!("not in enhance"),
    }
}

/// START's centre pixel: (380, 215).
const CX: i32 = 380;
const CY: i32 = 215;

#[test]
fn view_filters_live_in_the_lenses_and_never_bake_in() {
    let mut s = session();
    s.keys("e v");
    assert_eq!(s.view().filter, ViewFilter::Grey);
    assert!(s.status().ends_with("  grey"), "{}", s.status());
    s.keys("V");
    assert_eq!(s.view().filter, ViewFilter::Off);
    s.keys("V");
    assert_eq!(s.view().filter, ViewFilter::Contrast);
    s.keys("Escape");
    assert!(s.layers().is_empty() && s.open_layer().is_none());
    s.keys("v"); // in Select, v means nothing
    assert_eq!(s.view().filter, ViewFilter::Contrast);
}

#[test]
fn the_loupe_has_its_own_pixel_cursor_and_stays_in_the_box() {
    let mut s = session();
    s.keys("z");
    assert_eq!(s.mode_name(), "loupe"); // a toggle: still on after release
    assert_eq!(point(&s), PixelPoint::new(CX, CY));
    s.keys("l");
    assert_eq!(point(&s).x, CX + 1);
    s.press(9, "l", Mods::NONE, 0.0);
    s.tick(30.0); // held: a long way
    s.release(9, "l", Mods::NONE, 30.0);
    assert_eq!(point(&s).x, 659); // can't leave the box
    assert_eq!(s.rect(), START);
    s.keys("plus");
    assert_eq!(s.loupe_px(), 25);
    assert!(s.status().ends_with("  25px"));
    s.keys("z");
    assert_eq!(s.mode_name(), "select");
}

#[test]
fn shift_moves_the_loupe_one_loupe_width() {
    let mut s = session();
    s.keys("z H");
    assert_eq!(point(&s).x, CX - 21);
}

#[test]
fn autorepeat_does_not_flicker_the_loupe() {
    let mut s = session();
    s.press(7, "z", Mods::NONE, 0.0);
    s.press(7, "z", Mods::NONE, 0.3);
    s.press(7, "z", Mods::NONE, 0.4);
    assert_eq!(s.mode_name(), "loupe");
}

#[test]
fn a_censor_from_the_loupe_starts_at_the_loupe_and_esc_returns_there() {
    let mut s = session();
    s.keys(&format!("z {}", "h ".repeat(40)));
    let at = s.pixels().to_logical(point(&s));
    s.keys("x");
    assert_eq!(s.mode_name(), "censor");
    let Some(Mode::Censor(c)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(c.sel.rect().centre(), at);
    s.keys("Escape");
    assert_eq!(s.mode_name(), "loupe"); // back to where the layer opened
}

#[test]
fn the_loupe_opens_every_build_tool_at_its_pixel() {
    for (key, name) in [("x", "censor"), ("a", "annotate"), ("f", "filter")] {
        let mut s = session();
        s.keys(&format!("z {key}"));
        assert_eq!(s.mode_name(), name);
        assert!(s.open_layer().is_some());
    }
}

#[test]
fn a_loupe_over_a_tool_switches_tools_in_the_same_layer() {
    let mut s = session();
    s.keys("x t x z a"); // a bar locked; the loupe; then the pen
    assert_eq!(
        s.modes().iter().map(Mode::name).collect::<Vec<_>>(),
        ["annotate"]
    );
    assert_eq!(s.open_layer().unwrap().ops.len(), 2); // the bar, and the censor being edited
}

#[test]
fn a_loupe_through_enhance_still_locks_the_tool_below() {
    let names = |s: &Session| s.modes().iter().map(Mode::name).collect::<Vec<_>>();
    let mut s = session();
    s.keys("x t e z a"); // a bar being edited, under Enhance and the loupe
    assert_eq!(names(&s), ["annotate"]);
    assert_eq!(s.open_layer().unwrap().ops.len(), 1);

    let mut s = session();
    s.keys("a r l l j j e z x"); // a rect being drawn, likewise
    assert_eq!(names(&s), ["censor"]);
    let ops = &s.open_layer().unwrap().ops;
    assert!(
        matches!(ops[..], [ref o] if o.data.noun() == "shape"),
        "{ops:?}"
    );
}

#[test]
fn a_lens_drag_ends_under_the_next_lens() {
    let mut s = session();
    s.keys("e");
    s.button(Button::Left, p(300.0, 200.0), true);
    s.motion(p(320.0, 210.0));
    let dragged = lens(&s).lens.rect();
    s.keys("z"); // the drag ends here
    s.button(Button::Left, p(320.0, 210.0), false); // the release goes to the loupe
    s.keys("z");
    s.motion(p(500.0, 250.0)); // no button held
    assert_eq!(lens(&s).lens.rect(), dragged);
}

#[test]
fn the_loupe_follows_clicks_and_drags() {
    let mut s = session();
    s.keys("z");
    s.button(Button::Left, p(200.5, 190.5), true);
    assert_eq!(point(&s), PixelPoint::new(200, 190));
    s.motion(p(300.5, 200.5));
    assert_eq!(point(&s), PixelPoint::new(300, 200));
    s.button(Button::Left, p(300.5, 200.5), false);
    s.motion(p(400.5, 220.5)); // not dragging any more
    assert_eq!(point(&s), PixelPoint::new(300, 200));
    s.button(Button::Left, p(5000.0, 5000.0), true); // clamped to the box
    assert_eq!(point(&s), PixelPoint::new(659, 259));
}

#[test]
fn the_loupe_copies_its_pixel() {
    let mut s = session();
    s.keys("z l p");
    assert_eq!(
        s.outcome(),
        Some(&Outcome::Done(FinishedAction::Colour {
            pixel: PixelPoint::new(CX + 1, CY)
        }))
    );
}

#[test]
fn the_magnifier_stays_on_one_monitor_and_off_the_box() {
    let mut s = session_at(r(1700.0, 100.0, 200.0, 100.0), vec![]); // near the seam at 1920
    s.keys("z");
    let Some(Mode::Loupe(m)) = s.modes().last() else {
        panic!()
    };
    let g = s.magnifier(m);
    assert!(MONS.iter().any(|mon| mon.x <= g.x
        && g.right() <= mon.right()
        && mon.y <= g.y
        && g.bottom() <= mon.bottom()));
    assert!(!g.intersects(&s.rect()));
    assert_eq!((g.w, g.h), (210.0, 210.0)); // 21 pixels at 10 a pixel
}

#[test]
fn the_magnifier_stays_on_the_cursors_monitor_even_over_the_box() {
    let mut s = session_at(MONS[0], vec![]); // the box fills the left monitor
    s.keys("z");
    let Some(Mode::Loupe(m)) = s.modes().last() else {
        panic!()
    };
    let g = s.magnifier(m);
    assert!(MONS[0].x <= g.x && g.right() <= MONS[0].right(), "{g:?}");
    assert!(g.bottom() + screenie_core::lens::CHIP <= MONS[0].bottom());
}

#[test]
fn the_enhance_lens_zooms_and_stays_inside_the_box() {
    let mut s = session();
    s.keys("e");
    assert_eq!(s.mode_name(), "enhance");
    assert_eq!(lens(&s).lens.bounds(), START);
    assert_eq!(lens(&s).lens.rect(), r(240.0, 192.5, 280.0, 45.0)); // half the box, centred
    s.keys("plus");
    assert_eq!(lens(&s).zoom, 2.5);
    s.keys("minus");
    assert_eq!(lens(&s).zoom, 2.0);
    s.keys(&"l ".repeat(500)); // plain hjkl move the lens
    assert_eq!(lens(&s).lens.rect().right(), START.right()); // stopped at the box edge
    assert_eq!(s.rect(), START); // the box itself is frozen
    assert!(s.status().ends_with("  2.0x smooth"), "{}", s.status());
    s.keys("s");
    assert!(s.status().ends_with("  2.0x sharp"));
    s.keys("Escape");
    assert_eq!(s.mode_name(), "select");
}

#[test]
fn a_click_in_enhance_centres_the_lens_as_near_as_it_fits() {
    let mut s = session();
    s.keys("e click:200,200");
    assert_eq!(
        lens(&s).lens.rect().centre(),
        LogicalPoint::new(240.0, 200.0)
    );
}

#[test]
fn a_drag_in_enhance_draws_a_new_lens() {
    let mut s = session();
    s.keys("e drag:110,180:210,240");
    assert_eq!(lens(&s).lens.rect(), r(110.0, 180.0, 100.0, 60.0));
}

#[test]
fn the_wheel_taps_the_zoom() {
    let mut s = session();
    s.keys("e wheel:1");
    assert_eq!(lens(&s).zoom, 2.5);
}

#[test]
fn insert_moves_and_tools_open_at_its_cursor() {
    let mut s = session();
    s.keys("i");
    assert_eq!(s.mode_name(), "insert");
    assert_eq!(point(&s), PixelPoint::new(CX, CY));
    s.keys("l");
    assert_eq!(point(&s).x, CX + 5);
    s.keys("L"); // Shift: four steps
    assert_eq!(point(&s).x, CX + 25);
    s.keys("0");
    assert_eq!(point(&s).x, 100);
    s.keys("m G");
    assert_eq!(point(&s).y, 259);
    s.keys("m");
    let cursor = point(&s);
    s.keys("x"); // the cursor pixel is the censor's top-left; its own cursor bottom-right
    let Some(Mode::Censor(c)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(
        (c.sel.rect().x, c.sel.rect().y),
        (f64::from(cursor.x), f64::from(cursor.y))
    );
    assert_eq!(
        s.cursor_pixel(),
        Some(PixelPoint::new(cursor.x + 19, cursor.y + 19))
    );
    s.keys("l");
    let Some(Mode::Censor(c)) = s.modes().last() else {
        panic!()
    };
    assert_eq!(c.sel.rect().w, 25.0);
    s.keys("Return"); // commit: back to Insert, cursor unmoved
    assert_eq!(s.mode_name(), "insert");
    assert_eq!(point(&s), cursor);
    s.keys("x Escape"); // the layer away: back to Insert
    assert_eq!(s.mode_name(), "insert");
    s.keys("e"); // enhance centred on the cursor
    assert_eq!(lens(&s).lens.rect().centre(), s.pixels().to_logical(cursor));
    s.keys("Escape z"); // the loupe starts at the cursor, and the cursor follows it
    assert_eq!(point(&s), cursor);
    s.keys(&"h ".repeat(7)); // the loupe still moves 1px a tap
    s.keys("z");
    assert_eq!(s.mode_name(), "insert");
    assert_eq!(point(&s).x, CX - 7);
    s.keys("Escape");
    assert_eq!(s.mode_name(), "select");
}

#[test]
fn insert_alt_halves_toward_the_box_edge() {
    let mut s = session();
    s.keys("i <A-h>");
    assert_eq!(point(&s).x, (CX + 100) / 2);
    s.keys("<C-h> <A-S-h>"); // nothing for these here
    assert_eq!(point(&s).x, (CX + 100) / 2);
}

#[test]
fn plus_minus_in_insert_is_the_step_and_u_goes_back() {
    let mut s = session();
    s.keys("i plus");
    assert!(s.status().ends_with("  step 6px"));
    let x = point(&s).x;
    s.keys("l");
    assert_eq!(point(&s).x, x + 6);
    s.keys("u"); // back where it was
    assert_eq!(point(&s).x, x);
    s.keys(&"minus ".repeat(30));
    assert!(s.status().ends_with("  step 1px"));
}

#[test]
fn insert_edge_jumps_stop_at_window_edges_or_the_box() {
    let mut s = session_at(r(0.0, 0.0, 1920.0, 1080.0), vec![]);
    s.keys("i w"); // from 960: the window edge at 958 is behind; 1920 is the box's
    assert_eq!(point(&s).x, 1919);
    s.keys("b"); // left: the window's right edge (958), the pixel just inside it
    assert_eq!(point(&s).x, 958);
}

#[test]
fn insert_top_speed_scales_with_the_box() {
    let pixels = screenie_core::geometry::PixelMap::new(LAYOUT, 3840, 1080);
    let top = |rect, d: Dir| {
        let m = InsertMode::new(&pixels, rect);
        let held: DirSet = [d].into_iter().collect();
        m.rates(held, false).top * f64::from(m.step) // steps/s to px/s
    };
    assert_eq!(top(r(100.0, 100.0, 400.0, 50.0), Dir::Right), 400.0); // 1s across
    assert_eq!(top(r(100.0, 100.0, 400.0, 50.0), Dir::Down), 50.0); // 1s down
    assert_eq!(top(r(100.0, 100.0, 400.0, 30.0), Dir::Down), 40.0); // the floor
    assert_eq!(top(r(0.0, 0.0, 3000.0, 1000.0), Dir::Right), 800.0); // capped
}

#[test]
fn the_tag_tracks_the_active_cursor() {
    let mut s = session();
    s.keys("i");
    let at = point(&s);
    assert!(
        s.status()
            .starts_with(&format!("INSERT  560×90  ({},{})", at.x, at.y)),
        "{}",
        s.status()
    );
    s.keys("l");
    assert!(s.status().contains(&format!("({},{})", at.x + 5, at.y)));
    s.keys("z h");
    let lp = point(&s);
    assert!(
        s.status()
            .starts_with(&format!("LOUPE  560×90  ({},{})", lp.x, lp.y))
    );
    assert_eq!(insert_at(&s), PixelPoint::new(at.x + 5, at.y)); // until the loupe closes
}

#[test]
fn enter_and_right_click_back_out_of_the_views() {
    for keys in ["i", "z", "e", "i z"] {
        let mut s = session();
        s.keys(keys);
        let depth = s.modes().len();
        s.button(Button::Right, p(500.0, 500.0), true); // right click = Enter = back
        assert_eq!(s.modes().len(), depth - 1, "{keys}");
        assert_eq!(s.outcome(), None);
    }
}

#[test]
fn lenses_open_from_every_mode() {
    for keys in ["", "x", "f", "a", "a r", "i", "z", "e"] {
        for (lens, name) in [("z", "loupe"), ("e", "enhance")] {
            let mut s = session();
            s.keys(keys);
            if s.mode_name() == name {
                continue; // (the lens itself: z / e close or reopen)
            }
            s.keys(lens);
            assert_eq!(s.mode_name(), name, "{keys:?} then {lens}");
        }
    }
}

#[test]
fn q_quits_from_every_view() {
    for keys in ["i", "z", "e", "i z"] {
        let mut s = session();
        s.keys(&format!("{keys} q"));
        assert_eq!(s.outcome(), Some(&Outcome::Cancelled), "{keys}");
    }
}

#[test]
fn a_held_key_in_the_loupe_is_not_a_new_press_in_select() {
    let mut s = session();
    s.keys("z");
    s.press(7, "h", Mods::NONE, 0.0); // held in the loupe...
    s.keys("Escape");
    s.press(7, "h", Mods::NONE, 0.3); // ...its repeat arrives in Select
    assert_eq!(s.rect(), START);
    s.release(7, "h", Mods::NONE, 0.4);
    s.keys("h");
    assert_ne!(s.rect(), START);
}

fn loupe_at(s: &Session) -> (i32, i32) {
    match s.modes().last() {
        Some(Mode::Loupe(l)) => (l.cursor.at.x, l.cursor.at.y),
        _ => panic!("not in the loupe: {}", s.mode_name()),
    }
}

#[test]
fn the_loupe_reopens_where_it_was_left_for_this_run_only() {
    let mut s = session();
    s.keys("z");
    assert_eq!(loupe_at(&s), (380, 215)); // the box's centre
    s.keys("l l l j z"); // move it, leave it
    s.keys("l"); // the box changes meanwhile
    s.keys("z");
    assert_eq!(loupe_at(&s), (383, 216));
    let mut fresh = session(); // a new run starts at the centre again
    fresh.keys("z");
    assert_eq!(loupe_at(&fresh), (380, 215));
}

#[test]
fn a_remembered_loupe_spot_outside_the_box_is_clamped_into_it() {
    let mut s = session();
    s.keys("z");
    s.button(Button::Left, p(650.5, 250.5), true);
    s.button(Button::Left, p(650.5, 250.5), false);
    s.keys("z h h h h z"); // the box shrinks 20px from the right
    assert_eq!(loupe_at(&s), (639, 250));
}
