//! The captured frame: immutable pixels plus the mapping that places them
//! in the logical layout.

use std::sync::Arc;

use screenie_core::geometry::{LogicalRect, PixelMap, PixelPoint};

use crate::image::{Image, Rgb};

/// The frozen capture. Cloning shares the pixels; nothing can change them.
#[derive(Clone, Debug)]
pub struct Frame {
    image: Arc<Image>,
    map: PixelMap,
    layout: LogicalRect,
}

impl Frame {
    /// A capture of the whole `layout` (logical), at whatever scale its
    /// pixel size implies.
    pub fn new(image: Image, layout: LogicalRect) -> Self {
        let map = PixelMap::new(layout, image.width() as i32, image.height() as i32);
        Self {
            image: Arc::new(image),
            map,
            layout,
        }
    }

    /// The logical rectangle the frame covers.
    pub fn layout(&self) -> LogicalRect {
        self.layout
    }

    pub fn image(&self) -> &Image {
        &self.image
    }

    pub fn map(&self) -> &PixelMap {
        &self.map
    }

    /// The pixels of a logical rectangle, by the one rounding rule.
    pub fn crop(&self, r: LogicalRect) -> Image {
        self.image.crop(self.map.to_pixels(r))
    }

    pub fn rgb(&self, p: PixelPoint) -> Option<Rgb> {
        self.image.rgb(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenie_core::geometry::PixelRect;

    #[test]
    fn crops_use_the_rounding_rule_at_1_5x() {
        let f = Frame::new(
            Image::filled(1500, 900, Rgb(1, 2, 3)),
            LogicalRect::new(0.0, 0.0, 1000.0, 600.0),
        );
        let r = LogicalRect::new(101.0, 10.0, 100.0, 50.0);
        assert_eq!(f.map().to_pixels(r), PixelRect::new(152, 15, 150, 75));
        let c = f.crop(r);
        assert_eq!((c.width(), c.height()), (150, 75));
    }
}
