//! Desktop information: monitors and windows, as core's `Layout`.

pub mod hyprland;

use screenie_core::layout::Layout;

pub trait DesktopInfo {
    fn layout(&self) -> crate::Result<Layout>;
}
