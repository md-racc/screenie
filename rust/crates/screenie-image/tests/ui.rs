//! The overlay, drawn headlessly. Set SCREENIE_RENDER_OUT=DIR to also
//! write the pictures, to look at them.

use screenie_core::geometry::{LogicalPoint, LogicalRect, PixelPoint};
use screenie_image::ui::{Ui, View};
use screenie_image::{Frame, Image, Picture, Rgb, encode};

const MONS: [LogicalRect; 2] = [
    LogicalRect::new(0.0, 0.0, 1920.0, 1080.0),
    LogicalRect::new(1920.0, 0.0, 1920.0, 1080.0),
];
const START: LogicalRect = LogicalRect::new(100.0, 170.0, 560.0, 90.0);

fn font() -> Option<Vec<u8>> {
    [
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    ]
    .iter()
    .find_map(|p| std::fs::read(p).ok())
}

/// The prototype's synthetic frame: a dark grey desktop, at `scale`.
fn frame(scale: u32) -> Picture {
    let mut img = Image::filled(3840 * scale, 1080 * scale, Rgb(31, 31, 36));
    // A light block where the prototype draws its text, to see the box.
    let w = img.width() as usize;
    for y in 195 * scale as usize..260 * scale as usize {
        for x in 120 * scale as usize..600 * scale as usize {
            let i = (y * w + x) * 4;
            img.data_mut()[i..i + 3].copy_from_slice(&[217, 217, 217]);
        }
    }
    Picture::new(Frame::new(img, LogicalRect::new(0.0, 0.0, 3840.0, 1080.0)))
}

fn view(top: Vec<String>) -> View {
    View {
        rect: START,
        cursor: LogicalPoint::new(START.right(), START.bottom()),
        active: true,
        boxes: vec![],
        status: "SELECT  560×90  (659,259)".into(),
        top,
        monitors: MONS.to_vec(),
    }
}

fn save(name: &str, img: &Image) {
    if let Some(dir) = std::env::var_os("SCREENIE_RENDER_OUT") {
        let path = std::path::Path::new(&dir).join(name);
        std::fs::write(path, encode::png(img, 1).unwrap()).unwrap();
    }
}

fn at(img: &Image, x: i32, y: i32) -> Rgb {
    img.rgb(PixelPoint::new(x, y)).unwrap()
}

#[test]
fn select_on_the_left_monitor() {
    let Some(font) = font() else {
        eprintln!("no font: skipped");
        return;
    };
    let mut ui = Ui::new(&font).unwrap();
    let f = frame(1);
    let mut into = Image::new(1920, 1080);
    ui.draw(&f, 0, MONS[0], &view(vec![]), &mut into);
    save("select.png", &into);
    assert_eq!(at(&into, 10, 10), Rgb(16, 16, 18)); // dimmed outside the box
    assert_eq!(at(&into, 300, 220), Rgb(217, 217, 217)); // clear inside
    assert_eq!(at(&into, 300, 169), at(&into, 300, 168)); // the outline, above
    let outline = at(&into, 300, 169);
    assert!(outline.0 > 200, "white-ish outline: {outline:?}");
    assert_eq!(at(&into, 660, 260), Rgb(255, 255, 255)); // the cursor marker
    // The status chip sits below the box, near-opaque black with text.
    let chip = (264..284).flat_map(|y| (100..300).map(move |x| (x, y)));
    let (dark, light) = chip.fold((0, 0), |(d, l), (x, y)| {
        let p = at(&into, x, y);
        (d + usize::from(p.0 < 10), l + usize::from(p.0 > 200))
    });
    assert!(
        dark > 1500 && light > 100,
        "chip: {dark} dark, {light} light"
    );
}

#[test]
fn the_right_monitor_shows_only_dimmed_frame() {
    let Some(font) = font() else { return };
    let mut ui = Ui::new(&font).unwrap();
    let mut into = Image::new(1920, 1080);
    ui.draw(&frame(1), 1, MONS[1], &view(vec![]), &mut into);
    assert!(into.data().chunks_exact(4).all(|p| p[..3] == [16, 16, 18]));
}

#[test]
fn the_legend_goes_above_the_box_and_scale_2_is_crisp() {
    let Some(font) = font() else { return };
    let mut ui = Ui::new(&font).unwrap();
    let legend = vec![
        "o/Space O/S-Space corner  w b } { edges  0 $ g G far  c centre  + - scope  . last  u undo"
            .to_string(),
        "Enter capture  y text  p colour  Q code  P pin  Esc cancel  q quit".to_string(),
    ];
    let mut into = Image::new(3840, 2160);
    ui.draw(&frame(2), 0, MONS[0], &view(legend), &mut into);
    save("legend@2x.png", &into);
    // Two chip rows above the box (126..166 logical): the first is mostly
    // chip black, the gap between the rows is the dimmed frame.
    let dark = (200..1600).filter(|&x| at(&into, x, 270).0 < 10).count();
    assert!(dark > 1000, "{dark}");
    assert_eq!(at(&into, 1500, 291), Rgb(16, 16, 18));
    // The box's outline at 2x is 4 device px.
    assert!(at(&into, 600, 336).0 > 200 && at(&into, 600, 339).0 > 200);
    assert_eq!(at(&into, 600, 335), Rgb(16, 16, 18));
}

fn session(start: LogicalRect) -> screenie_core::session::Session {
    use screenie_core::layout::{Layout, Monitor, MonitorId};
    let monitors = MONS
        .iter()
        .enumerate()
        .map(|(i, &rect)| Monitor {
            id: MonitorId(i as u32),
            name: format!("DP-{i}"),
            rect,
            scale: 1.0,
        })
        .collect();
    screenie_core::session::Session::new(screenie_core::session::Setup {
        layout: Layout::new(monitors, vec![], MonitorId(0)),
        pixels: screenie_core::geometry::PixelMap::new(
            LogicalRect::new(0.0, 0.0, 3840.0, 1080.0),
            3840,
            1080,
        ),
        start: Some(start),
        recall: vec![],
    })
}

#[test]
fn a_censor_shows_while_open_and_burns_in_on_commit() {
    let Some(font) = font() else { return };
    let mut ui = Ui::new(&font).unwrap();
    let mut pic = frame(1);
    let mut s = session(START);
    s.play("x t", 0.0).unwrap(); // a black bar at the centre of the box
    pic.sync(s.layers(), s.open_layer(), s.editing().as_ref());
    let mut into = Image::new(1920, 1080);
    ui.draw(&pic, 0, MONS[0], &View::of(&s), &mut into);
    save("censor.png", &into);
    assert_eq!(at(&into, 380, 215), Rgb(0, 0, 0)); // the bar, shown
    assert!(at(&into, 380, 203).0 > 200); // its outline, above it
    assert_eq!(
        pic.crop(START).rgb(PixelPoint::new(280, 45)),
        Some(Rgb(217, 217, 217))
    ); // not output yet
    s.play("Return", 0.0).unwrap();
    pic.sync(s.layers(), s.open_layer(), s.editing().as_ref());
    assert_eq!(
        pic.crop(START).rgb(PixelPoint::new(280, 45)),
        Some(Rgb(0, 0, 0))
    ); // committed
    s.play("u", 0.0).unwrap();
    pic.sync(s.layers(), s.open_layer(), s.editing().as_ref());
    assert_eq!(
        pic.crop(START).rgb(PixelPoint::new(280, 45)),
        Some(Rgb(217, 217, 217))
    ); // undone
}

#[test]
fn a_project_serialized_and_read_back_renders_the_same_pixels() {
    use screenie_core::model::Project;
    let mut s = session(START);
    // A pixelate, a black bar locked over it, both committed; then a
    // second layer with a white bar.
    s.play("x <S-l> <S-l> x t <S-h> Return x t plus plus plus plus plus plus plus plus plus plus Return", 0.0)
        .unwrap();
    let project = s.project();
    assert_eq!(project.layers.len(), 2);
    let back = Project::from_json(&project.to_json()).unwrap();
    assert_eq!(back, project);
    let render = |p: &Project| {
        let mut pic = frame(1);
        pic.sync(&p.layers, None, None);
        pic.crop(p.selection)
    };
    let (a, b) = (render(&project), render(&back));
    assert_eq!(a, b);
    assert_ne!(a, frame(1).crop(START)); // and the layers did something
}
