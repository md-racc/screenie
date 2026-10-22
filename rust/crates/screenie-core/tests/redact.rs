//! Auto-redact's review (Censor's `r`): scanning as a job, ticking, and
//! accepting the ticked suggestions as censors in one undo step.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use screenie_core::geometry::PixelRect;
use screenie_core::model::{CensorKind, OpData};
use screenie_core::ocr::{OcrResult, OcrWord};
use screenie_core::session::{Analyzer, JobKind, JobResult, Session};

/// One line of words at the box's top-left, 10px a character.
fn words(ws: &[&str]) -> OcrResult {
    let mut x = 110;
    let words = ws
        .iter()
        .map(|w| {
            let n = w.chars().count() as i32;
            let word = OcrWord {
                text: w.to_string(),
                bbox: PixelRect::new(x, 200, n * 10, 12),
                confidence: Some(90.0),
                line: 0,
            };
            x += n * 10 + 10;
            word
        })
        .collect();
    OcrResult {
        text: ws.join(" "),
        words,
        codes: vec![],
    }
}

/// Answers at once, as for `--keys` and `--no-ui`.
struct Sync(OcrResult);

impl Analyzer for Sync {
    fn start(&mut self, _: &Session, _: u64, job: JobKind) -> Option<JobResult> {
        let JobKind::Ocr { rect, .. } = job;
        assert_eq!(rect, PixelRect::new(100, 170, 560, 90)); // the box
        Some(JobResult::Ocr(Ok(self.0.clone())))
    }
}

/// Answers later: the jobs it was given.
struct Later(Rc<RefCell<Vec<u64>>>);

impl Analyzer for Later {
    fn start(&mut self, _: &Session, id: u64, _: JobKind) -> Option<JobResult> {
        self.0.borrow_mut().push(id);
        None
    }
}

fn secrets() -> OcrResult {
    // An email (ticked) and a random-looking string (unticked: it's not sure).
    words(&["adi@example.org", "id", "aB3dE6gH9jK2aB3dE6gH9jK2"])
}

fn censors(s: &Session) -> Vec<CensorKind> {
    s.open_layer()
        .map(|l| {
            l.ops
                .iter()
                .map(|o| match &o.data {
                    OpData::Censor(c) => c.kind,
                    other => panic!("not a censor: {other:?}"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn found(s: &Session) -> Vec<(&'static str, bool)> {
    s.suggestions()
        .map(|(l, _)| l.iter().map(|x| (x.category.name(), x.ticked)).collect())
        .unwrap_or_default()
}

#[test]
fn r_scans_the_box_and_lists_what_it_found() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x r");
    assert_eq!(s.mode_name(), "redact");
    let f = found(&s);
    assert_eq!(f.len(), 2, "{f:?}");
    assert_eq!(f.iter().filter(|x| x.1).count(), 1);
    assert!(s.status().contains("2 found (1 ticked)"), "{}", s.status());
}

#[test]
fn accepting_makes_black_bars_in_one_undo_step() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x r bracketleft space"); // tick the second, so both are ticked
    assert!(s.status().contains("(2 ticked)"), "{}", s.status());
    s.keys("a");
    assert_eq!(s.mode_name(), "censor");
    assert_eq!(censors(&s), [CensorKind::Bar, CensorKind::Bar]);
    assert_eq!(s.redacted().len(), 2);
    s.keys("u");
    assert!(censors(&s).is_empty(), "one undo takes both back");
}

#[test]
fn accepting_with_kind_pixelate_makes_pixelate_censors() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.set_redact_rules(screenie_core::redact::Rules {
        kind: CensorKind::Pixelate,
        ..Default::default()
    });
    s.keys("x r Return");
    let ops: Vec<(CensorKind, f64)> = s
        .open_layer()
        .unwrap()
        .ops
        .iter()
        .map(|o| match &o.data {
            OpData::Censor(c) => (c.kind, c.amount),
            other => panic!("not a censor: {other:?}"),
        })
        .collect();
    let block = screenie_core::censor::PIXELATE_BLOCK;
    assert_eq!(ops, [(CensorKind::Pixelate, block)]);
}

#[test]
fn only_the_ticked_are_accepted_and_esc_drops_them_all() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x r Return");
    assert_eq!(censors(&s).len(), 1);
    s.keys("r Escape");
    assert_eq!(s.mode_name(), "censor");
    assert_eq!(censors(&s).len(), 1);
}

#[test]
fn d_drops_a_suggestion() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x r d");
    assert_eq!(found(&s).len(), 1);
    s.keys("d d");
    assert!(found(&s).is_empty());
    assert!(s.status().contains("0 found"));
}

#[test]
fn suggestions_stay_inside_the_box() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x r");
    for x in s.suggestions().unwrap().0 {
        let r = x.rect;
        let inside = START.x <= r.x
            && START.y <= r.y
            && r.right() <= START.right()
            && r.bottom() <= START.bottom();
        assert!(inside, "{r:?}");
    }
}

#[test]
fn a_scan_in_flight_keeps_the_picker_live_and_late_results_are_dropped() {
    let jobs = Rc::new(RefCell::new(vec![]));
    let mut s = session();
    s.set_analyzer(Box::new(Later(jobs.clone())));
    s.keys("x r");
    assert!(s.status().contains("scanning…"));
    assert!(s.wants_ticks());
    s.keys("r"); // rescan: the first job is stale
    let (first, second) = (jobs.borrow()[0], jobs.borrow()[1]);
    s.job_done(first, JobResult::Ocr(Ok(secrets())));
    assert!(s.status().contains("scanning…"));
    s.job_done(second, JobResult::Ocr(Ok(secrets())));
    assert_eq!(found(&s).len(), 2);
    assert!(!s.wants_ticks());
    // Gone from the review, a result has nowhere to go.
    s.keys("r Escape");
    let third = jobs.borrow()[2];
    s.job_done(third, JobResult::Ocr(Ok(secrets())));
    assert_eq!(s.mode_name(), "censor");
}

#[test]
fn a_failed_scan_says_why() {
    let mut s = session();
    s.set_analyzer(Box::new(Later(Rc::default())));
    s.keys("x r");
    s.job_done(1, JobResult::Ocr(Err("tesseract not found".into())));
    assert!(s.status().contains("scan failed: tesseract not found"));
}

#[test]
fn without_an_analyzer_the_scan_fails_plainly() {
    let mut s = session();
    s.keys("x r");
    assert!(s.status().contains("no OCR available"), "{}", s.status());
}

#[test]
fn the_legend_and_list_come_from_the_review() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.keys("x");
    assert!(s.legend().join(" ").contains("find secrets"));
    s.keys("r");
    assert!(s.legend().join(" ").contains("accept ticked"));
    let lists = s.lists();
    let rows = lists.last().unwrap();
    assert!(
        rows[0].starts_with('>') && rows[0].contains("email"),
        "{rows:?}"
    );
}

#[test]
fn after_a_drop_the_next_row_down_is_selected() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(words(&[
        "adi@example.org",
        "10.0.0.12",
        "aa:bb:cc:dd:ee:ff",
    ]))));
    s.keys("x r bracketleft d");
    let (l, sel) = s.suggestions().unwrap();
    assert_eq!(l[sel].category.name(), "mac");
    s.keys("d");
    let (l, sel) = s.suggestions().unwrap();
    assert_eq!(l[sel].category.name(), "email"); // none below: the one above
}

#[test]
fn redact_auto_commits_the_ticked_as_a_layer() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.start_redact(true).unwrap();
    assert_eq!(s.mode_name(), "select");
    assert_eq!(s.layers().len(), 1);
    assert_eq!(s.layers()[0].ops.len(), 1); // only the ticked email
    assert_eq!(s.redacted().len(), 1);
    assert_eq!(s.redacted()[0].0, "email");
}

#[test]
fn redact_suggest_leaves_the_review_open_and_undone_ones_are_not_reported() {
    let mut s = session();
    s.set_analyzer(Box::new(Sync(secrets())));
    s.start_redact(false).unwrap();
    assert_eq!(s.mode_name(), "redact");
    s.keys("a");
    assert_eq!(s.redacted().len(), 1);
    s.keys("u");
    assert!(s.redacted().is_empty());
}

#[test]
fn redact_auto_fails_when_the_scan_does() {
    let mut s = session();
    assert_eq!(s.start_redact(true), Err("no OCR available".into()));
}
