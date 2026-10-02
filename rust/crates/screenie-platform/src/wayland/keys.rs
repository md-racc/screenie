//! xkb keysyms as core keys.

use screenie_core::keys::Key;
use smithay_client_toolkit::seat::keyboard::Keysym;

pub(crate) fn key(sym: Keysym) -> Key {
    match sym {
        Keysym::Return | Keysym::KP_Enter => Key::Return,
        Keysym::Escape => Key::Escape,
        Keysym::space | Keysym::KP_Space => Key::Space,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::BackSpace => Key::BackSpace,
        Keysym::Delete | Keysym::KP_Delete => Key::Delete,
        Keysym::Left | Keysym::KP_Left => Key::Left,
        Keysym::Right | Keysym::KP_Right => Key::Right,
        Keysym::Up | Keysym::KP_Up => Key::Up,
        Keysym::Down | Keysym::KP_Down => Key::Down,
        Keysym::Shift_L | Keysym::Shift_R => Key::Shift,
        Keysym::Control_L | Keysym::Control_R => Key::Control,
        Keysym::Alt_L | Keysym::Alt_R | Keysym::Meta_L | Keysym::Meta_R => Key::Alt,
        Keysym::Super_L
        | Keysym::Super_R
        | Keysym::ISO_Level3_Shift
        | Keysym::ISO_Level5_Shift
        | Keysym::Caps_Lock
        | Keysym::Num_Lock => Key::Super,
        Keysym::KP_Add => Key::Char('+'),
        Keysym::KP_Subtract => Key::Char('-'),
        _ => match sym.key_char() {
            Some(c) if !c.is_control() => Key::Char(c),
            _ => Key::Other,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms() {
        assert_eq!(key(Keysym::h), Key::Char('h'));
        assert_eq!(key(Keysym::O), Key::Char('O'));
        assert_eq!(key(Keysym::dollar), Key::Char('$'));
        assert_eq!(key(Keysym::braceright), Key::Char('}'));
        assert_eq!(key(Keysym::Return), Key::Return);
        assert_eq!(key(Keysym::space), Key::Space);
        assert_eq!(key(Keysym::KP_Add), Key::Char('+'));
        assert_eq!(key(Keysym::KP_7), Key::Char('7'));
        assert_eq!(key(Keysym::Shift_R), Key::Shift);
        assert_eq!(key(Keysym::F1), Key::Other);
    }
}
