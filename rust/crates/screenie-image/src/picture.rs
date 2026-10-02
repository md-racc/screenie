//! What the picker shows and outputs, from the immutable capture and the
//! project: the committed picture (every visible layer applied in order)
//! and, while a layer is open, its view over the layer's clip.
//!
//! Everything here is a cache. Throwing it away changes nothing: `sync`
//! rebuilds what's needed from the capture and the layers.

use screenie_core::geometry::{LogicalRect, PixelPoint, PixelRect};
use screenie_core::model::{Layer, Op, OpData};

use crate::frame::Frame;
use crate::image::{Image, Rgb};
use crate::render::{Patch, apply_layer, apply_op};

pub struct Picture {
    base: Frame,
    committed: Frame,
    /// The layers `committed` was made from.
    made_from: Vec<Layer>,
    version: u64,
    /// The open layer's clip and locked ops, applied over the committed
    /// picture in that clip.
    locked: Option<(LogicalRect, Vec<Op>, PixelRect, Image)>,
    /// The same, plus the item being edited: what's shown.
    shown: Option<(Option<OpData>, Image)>,
}

impl Picture {
    pub fn new(base: Frame) -> Self {
        Self {
            committed: base.clone(),
            base,
            made_from: Vec::new(),
            version: 0,
            locked: None,
            shown: None,
        }
    }

    /// The capture, untouched.
    pub fn base(&self) -> &Frame {
        &self.base
    }

    /// The committed picture.
    pub fn committed(&self) -> &Frame {
        &self.committed
    }

    /// Bumps whenever the committed picture changes.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Bring the caches up to date with the session.
    pub fn sync(&mut self, layers: &[Layer], open: Option<&Layer>, editing: Option<&OpData>) {
        if layers != self.made_from.as_slice() {
            let map = *self.base.map();
            let mut img = self.base.image().clone();
            for layer in layers.iter().filter(|l| !l.hidden) {
                apply_layer(
                    &mut Patch {
                        img: &mut img,
                        origin: PixelPoint::new(0, 0),
                    },
                    &map,
                    layer,
                );
            }
            self.committed = Frame::new(img, self.base.layout());
            self.made_from = layers.to_vec();
            self.version += 1;
            self.locked = None;
            self.shown = None;
        }
        let Some(open) = open else {
            self.locked = None;
            self.shown = None;
            return;
        };
        let fresh = self
            .locked
            .as_ref()
            .is_some_and(|(clip, ops, _, _)| *clip == open.clip && *ops == open.ops);
        if !fresh {
            let map = *self.committed.map();
            let ext = self.committed.image().extent();
            let px = map.to_pixels(open.clip);
            let x0 = px.x.max(0);
            let y0 = px.y.max(0);
            let r = PixelRect::new(
                x0,
                y0,
                ((px.x + px.w).min(ext.w) - x0).max(0),
                ((px.y + px.h).min(ext.h) - y0).max(0),
            );
            let mut img = self.committed.image().crop(r);
            apply_layer(
                &mut Patch {
                    img: &mut img,
                    origin: PixelPoint::new(r.x, r.y),
                },
                &map,
                open,
            );
            self.locked = Some((open.clip, open.ops.clone(), r, img));
            self.shown = None;
        }
        let editing = editing.cloned();
        if self.shown.as_ref().is_some_and(|(e, _)| *e == editing) {
            return;
        }
        let (_, _, r, locked) = self.locked.as_ref().expect("just made");
        let mut img = locked.clone();
        if let Some(op) = &editing {
            let map = *self.committed.map();
            let clip = map.to_pixels(open.clip);
            apply_op(
                &mut Patch {
                    img: &mut img,
                    origin: PixelPoint::new(r.x, r.y),
                },
                &map,
                clip,
                op,
            );
        }
        self.shown = Some((editing, img));
    }

    /// The open layer's view over its clip (frame px), if a layer is open.
    pub fn open_patch(&self) -> Option<(PixelRect, &Image)> {
        let (_, _, r, _) = self.locked.as_ref()?;
        let (_, img) = self.shown.as_ref()?;
        Some((*r, img))
    }

    /// What's shown at a pixel: the open layer's view inside its clip, else
    /// the committed picture.
    pub fn rgb(&self, p: PixelPoint) -> Option<Rgb> {
        if let Some((r, img)) = self.open_patch() {
            let inside = p.x >= r.x && p.y >= r.y && p.x < r.x + r.w && p.y < r.y + r.h;
            if inside {
                return img.rgb(PixelPoint::new(p.x - r.x, p.y - r.y));
            }
        }
        self.committed.rgb(p)
    }

    /// The output for a box: the committed picture's pixels.
    pub fn crop(&self, rect: LogicalRect) -> Image {
        self.committed.crop(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::model::{CensorKind, CensorOp, LayerId, OpId};

    const ALL: LogicalRect = LogicalRect::new(0.0, 0.0, 100.0, 50.0);

    fn picture() -> Picture {
        Picture::new(Frame::new(Image::filled(100, 50, Rgb(10, 10, 10)), ALL))
    }

    fn bar(id: u64, x: f64) -> Op {
        Op {
            id: OpId(id),
            data: OpData::Censor(CensorOp {
                rect: LogicalRect::new(x, 0.0, 10.0, 10.0),
                kind: CensorKind::Bar,
                amount: 1.0,
            }),
        }
    }

    fn layer(id: u64, ops: Vec<Op>) -> Layer {
        Layer {
            id: LayerId(id),
            clip: LogicalRect::new(0.0, 0.0, 50.0, 50.0),
            hidden: false,
            ops,
        }
    }

    fn at(p: &Picture, x: i32) -> Rgb {
        p.rgb(PixelPoint::new(x, 5)).unwrap()
    }

    #[test]
    fn committed_layers_change_the_output_and_hidden_ones_do_not() {
        let mut p = picture();
        let mut l = layer(1, vec![bar(2, 0.0)]);
        p.sync(std::slice::from_ref(&l), None, None);
        assert_eq!(p.version(), 1);
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(255, 255, 255))
        );
        l.hidden = true;
        p.sync(std::slice::from_ref(&l), None, None);
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(10, 10, 10))
        );
        assert_eq!(p.base().rgb(PixelPoint::new(5, 5)), Some(Rgb(10, 10, 10))); // never touched
    }

    #[test]
    fn the_open_layer_is_shown_but_not_output() {
        let mut p = picture();
        let open = layer(1, vec![bar(2, 0.0)]);
        let editing = bar(3, 20.0).data;
        p.sync(&[], Some(&open), Some(&editing));
        assert_eq!(at(&p, 5), Rgb(255, 255, 255)); // locked
        assert_eq!(at(&p, 25), Rgb(255, 255, 255)); // being edited
        assert_eq!(at(&p, 45), Rgb(10, 10, 10));
        assert_eq!(
            p.crop(ALL).rgb(PixelPoint::new(5, 5)),
            Some(Rgb(10, 10, 10))
        );
        p.sync(&[], None, None); // discarded
        assert!(p.open_patch().is_none());
        assert_eq!(at(&p, 5), Rgb(10, 10, 10));
    }

    #[test]
    fn the_open_layer_is_clipped() {
        let mut p = picture();
        let open = layer(1, vec![]);
        p.sync(&[], Some(&open), Some(&bar(3, 45.0).data));
        assert_eq!(at(&p, 48), Rgb(255, 255, 255));
        assert_eq!(at(&p, 51), Rgb(10, 10, 10)); // past the 50px clip
    }

    #[test]
    fn rebuilding_from_scratch_gives_the_same_pixels() {
        let layers = vec![layer(1, vec![bar(2, 0.0), bar(3, 30.0)])];
        let mut a = picture();
        a.sync(&layers, None, None);
        let mut b = picture();
        b.sync(&layers, Some(&layer(9, vec![])), None); // caches in another state
        b.sync(&layers, None, None);
        assert_eq!(a.crop(ALL), b.crop(ALL));
    }
}
