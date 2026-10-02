//! Measure's numbers: real units, typed lengths ("5 m", `12'6"`), the
//! scale and its error, and how values are written (docs/measure.md).

/// A unit of length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Mm,
    Cm,
    M,
    Km,
    In,
    Ft,
    Yd,
    Mi,
}

impl Unit {
    pub const ALL: [Unit; 8] = [
        Unit::Mm,
        Unit::Cm,
        Unit::M,
        Unit::Km,
        Unit::In,
        Unit::Ft,
        Unit::Yd,
        Unit::Mi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::Km => "km",
            Unit::In => "in",
            Unit::Ft => "ft",
            Unit::Yd => "yd",
            Unit::Mi => "mi",
        }
    }

    /// Metres in one of it.
    pub fn metres(self) -> f64 {
        match self {
            Unit::Mm => 0.001,
            Unit::Cm => 0.01,
            Unit::M => 1.0,
            Unit::Km => 1000.0,
            Unit::In => 0.0254,
            Unit::Ft => 0.3048,
            Unit::Yd => 0.9144,
            Unit::Mi => 1609.344,
        }
    }

    pub fn metric(self) -> bool {
        matches!(self, Unit::Mm | Unit::Cm | Unit::M | Unit::Km)
    }

    /// `t`: the other system's counterpart.
    pub fn other(self) -> Unit {
        match self {
            Unit::Mm | Unit::Cm => Unit::In,
            Unit::M => Unit::Ft,
            Unit::Km => Unit::Mi,
            Unit::In => Unit::Cm,
            Unit::Ft | Unit::Yd => Unit::M,
            Unit::Mi => Unit::Km,
        }
    }

    fn parse(s: &str) -> Option<Unit> {
        match s {
            "'" => Some(Unit::Ft),
            "\"" => Some(Unit::In),
            _ => Unit::ALL.into_iter().find(|u| u.name() == s),
        }
    }
}

/// A typed length: a value in a unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    pub value: f64,
    pub unit: Unit,
}

/// `5 m`, `2.5km`, `2*2.5 m`, `12'6"`, `12ft 6in`: a number (or a little
/// arithmetic) with a unit, or several summed, in the first one's unit.
pub fn parse_length(s: &str) -> Result<Length, String> {
    let bad = || format!("can't read {:?}: a number and a unit, like 5 m", s.trim());
    let mut parts: Vec<(f64, Unit)> = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        // The number: everything up to the first unit letter or mark.
        let end = rest
            .find(|c: char| c.is_ascii_alphabetic() || c == '\'' || c == '"')
            .ok_or_else(bad)?;
        let value = eval(&rest[..end]).ok_or_else(bad)?;
        let tail = &rest[end..];
        let ulen = if tail.starts_with(['\'', '"']) {
            1
        } else {
            tail.find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(tail.len())
        };
        let unit = Unit::parse(&tail[..ulen]).ok_or_else(bad)?;
        parts.push((value, unit));
        rest = tail[ulen..].trim_start();
    }
    let (_, unit) = *parts.first().ok_or_else(bad)?;
    let metres: f64 = parts.iter().map(|(v, u)| v * u.metres()).sum();
    if !(metres > 0.0 && metres.is_finite()) {
        return Err(format!("{:?}: a length must be more than zero", s.trim()));
    }
    Ok(Length {
        value: metres / unit.metres(),
        unit,
    })
}

/// `+ - * /` over numbers, with the usual precedence.
fn eval(s: &str) -> Option<f64> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut term = String::new();
    let flush = |term: &str, sign: f64, total: &mut f64| -> Option<()> {
        let mut v: Option<f64> = None;
        let mut op = '*';
        let mut num = String::new();
        for c in term.chars().chain(std::iter::once('*')) {
            if c == '*' || c == '/' {
                let n: f64 = num.parse().ok()?;
                v = Some(match (v, op) {
                    (None, _) => n,
                    (Some(a), '*') => a * n,
                    (Some(a), _) => a / n,
                });
                op = c;
                num.clear();
            } else {
                num.push(c);
            }
        }
        *total += sign * v?;
        Some(())
    };
    for (i, c) in s.chars().enumerate() {
        if (c == '+' || c == '-') && i > 0 {
            flush(&term, sign, &mut total)?;
            term.clear();
            sign = if c == '+' { 1.0 } else { -1.0 };
        } else {
            term.push(c);
        }
    }
    flush(&term, sign, &mut total)?;
    Some(total)
}

/// The calibration: `px` capture pixels are `real` of `unit`, and the
/// share of error in that (from the reference's points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub px: f64,
    pub real: f64,
    pub unit: Unit,
    pub error: f64,
}

/// Each placed point is good to about this, in capture pixels.
pub const POINT_ERROR: f64 = 1.0;
/// A reference worse than this suggests measuring a longer one.
pub const SHORT_REFERENCE: f64 = 0.02;

impl Scale {
    /// From a reference of `points` points, `px` long.
    pub fn new(px: f64, real: Length, points: usize) -> Scale {
        Scale {
            px,
            real: real.value,
            unit: real.unit,
            error: (points as f64).sqrt() * POINT_ERROR / px,
        }
    }

    /// Capture pixels in one metre.
    pub fn px_per_metre(&self) -> f64 {
        self.px / (self.real * self.unit.metres())
    }

    /// `--measure-scale '84.2px=1m'`.
    pub fn parse(s: &str) -> Result<Scale, String> {
        let bad = || format!("bad scale {s:?}: like 84.2px=1m");
        let (px, real) = s.split_once('=').ok_or_else(bad)?;
        let px: f64 = px
            .trim()
            .strip_suffix("px")
            .ok_or_else(bad)?
            .trim()
            .parse()
            .map_err(|_| bad())?;
        if px.is_nan() || px <= 0.0 {
            return Err(bad());
        }
        let real = parse_length(real)?;
        Ok(Scale {
            px,
            real: real.value,
            unit: real.unit,
            error: 0.0,
        })
    }

    /// `1 m = 84.2 px ±1.7%`.
    pub fn describe(&self) -> String {
        let one = format!(
            "1 {} = {} px",
            self.unit.name(),
            fixed(self.px / self.real, 1)
        );
        if self.error > 0.0 {
            format!("{one} ±{}%", fixed(self.error * 100.0, 1))
        } else {
            one
        }
    }
}

/// How values are shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Display {
    Pixels,
    Real(Unit),
}

/// `v` to three significant figures, but never in exponent form.
pub fn sig3(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let digits = v.abs().log10().floor() as i32;
    let decimals = (2 - digits).max(0) as usize;
    format!("{v:.decimals$}")
}

fn fixed(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}")
}

/// `±err` to the value's decimals, or more so it shows a digit.
fn plus_minus(text: &str, err: f64) -> String {
    let decimals = text.split_once('.').map_or(0, |(_, d)| d.len());
    let needed = if err > 0.0 {
        (-err.log10().floor()).max(0.0) as usize
    } else {
        0
    };
    let decimals = decimals.max(needed.min(6));
    format!("±{err:.decimals$}")
}

/// A length `px` long, measured from `points` points: `4.20 m ±0.08`, or
/// `352.1 px`.
pub fn length(px: f64, points: usize, scale: Option<&Scale>, d: Display) -> String {
    match (d, scale) {
        (Display::Real(unit), Some(s)) => {
            let ppu = s.px_per_metre() * unit.metres();
            let v = px / ppu;
            let own = (points as f64).sqrt() * POINT_ERROR / ppu;
            let err = ((v * s.error).powi(2) + own.powi(2)).sqrt();
            let text = sig3(v);
            format!("{text} {} {}", unit.name(), plus_minus(&text, err))
        }
        _ => format!("{} px", fixed(px, 1)),
    }
}

/// A length without its error, for details beside one that has it.
pub fn length_bare(px: f64, scale: Option<&Scale>, d: Display) -> String {
    match (d, scale) {
        (Display::Real(unit), Some(s)) => {
            let ppu = s.px_per_metre() * unit.metres();
            format!("{} {}", sig3(px / ppu), unit.name())
        }
        _ => format!("{} px", fixed(px, 1)),
    }
}

/// An area of `px2` square pixels, with its perimeter's pixels for the
/// error: `12.6 m² ±0.5`, or `1234 px²`.
pub fn area(px2: f64, perimeter_px: f64, scale: Option<&Scale>, d: Display) -> String {
    match (d, scale) {
        (Display::Real(unit), Some(s)) => {
            let ppu = s.px_per_metre() * unit.metres();
            let v = px2 / (ppu * ppu);
            let own = perimeter_px * POINT_ERROR / 2.0 / (ppu * ppu);
            let err = ((2.0 * v * s.error).powi(2) + own.powi(2)).sqrt();
            let text = sig3(v);
            format!("{text} {}² {}", unit.name(), plus_minus(&text, err))
        }
        _ => format!("{} px²", fixed(px2, 0)),
    }
}

/// Degrees: `37.5°`.
pub fn degrees(d: f64) -> String {
    format!("{}°", fixed(d, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len(s: &str) -> (f64, &'static str) {
        let l = parse_length(s).unwrap();
        ((l.value * 1e6).round() / 1e6, l.unit.name())
    }

    #[test]
    fn typed_lengths() {
        assert_eq!(len("5 m"), (5.0, "m"));
        assert_eq!(len("2.5km"), (2.5, "km"));
        assert_eq!(len(" 2*2.5 m "), (5.0, "m"));
        assert_eq!(len("10/4 ft"), (2.5, "ft"));
        assert_eq!(len("1+2*3 cm"), (7.0, "cm"));
        assert_eq!(len("12'6\""), (12.5, "ft"));
        assert_eq!(len("12ft 6in"), (12.5, "ft"));
        assert_eq!(len("1 m 50 cm"), (1.5, "m"));
        for bad in ["", "5", "m", "5 parsecs", "0 m", "-2 m", "five m"] {
            assert!(parse_length(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_scale_and_its_error() {
        // The spec's example: an 80 px reference is good to about ±1.8%.
        let s = Scale::new(80.0, parse_length("1 m").unwrap(), 2);
        assert!((s.error - 0.0177).abs() < 1e-3);
        assert_eq!(s.describe(), "1 m = 80.0 px ±1.8%");
        let s = Scale::parse("84.2px=1m").unwrap();
        assert_eq!((s.px, s.real, s.unit), (84.2, 1.0, Unit::M));
        assert_eq!(s.describe(), "1 m = 84.2 px");
        assert!(Scale::parse("84.2=1m").is_err());
        assert!(Scale::parse("0px=1m").is_err());
    }

    #[test]
    fn values_as_written() {
        assert_eq!(sig3(4.2), "4.20");
        assert_eq!(sig3(12.6), "12.6");
        assert_eq!(sig3(411.0), "411");
        assert_eq!(sig3(0.0123), "0.0123");
        let s = Scale::new(411.0, parse_length("5 m").unwrap(), 2);
        let m = Display::Real(Unit::M);
        assert_eq!(length(352.1, 2, Some(&s), m), "4.28 m ±0.02");
        assert_eq!(length(352.1, 2, Some(&s), Display::Pixels), "352.1 px");
        assert_eq!(length(352.1, 2, None, m), "352.1 px");
        assert!(length(352.1, 2, Some(&s), Display::Real(Unit::Ft)).starts_with("14.1 ft"));
        assert!(area(10000.0, 400.0, Some(&s), m).starts_with("1.48 m²"));
        assert_eq!(area(1234.4, 0.0, None, m), "1234 px²");
        assert_eq!(degrees(37.54), "37.5°");
        // An error too small for the value's decimals still shows a digit.
        let fine = Scale::parse("50px=1m").unwrap();
        assert_eq!(length(1915.0, 2, Some(&fine), m), "38.3 m ±0.03");
        assert_eq!(length_bare(1915.0, Some(&fine), m), "38.3 m");
    }

    #[test]
    fn t_switches_system() {
        assert_eq!(Unit::M.other(), Unit::Ft);
        assert_eq!(Unit::Ft.other(), Unit::M);
        assert_eq!(Unit::Km.other().other(), Unit::Km);
    }
}
