//! Measure's numbers: real units, typed lengths ("5 m", `12'6"`), the
//! scale and its error, and how values are written (docs/measure.md).

/// A name for a unit that isn't a standard length (`u`, `ly`, `bricks`):
/// kept inline, so units stay plain values. Longer names are cut.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    len: u8,
    bytes: [u8; Tag::MAX],
}

impl Tag {
    pub const MAX: usize = 24;

    pub fn new(name: &str) -> Tag {
        let mut end = name.len().min(Tag::MAX);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = [0; Tag::MAX];
        bytes[..end].copy_from_slice(&name.as_bytes()[..end]);
        Tag {
            len: end as u8,
            bytes,
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("cut on a char boundary")
    }
}

impl std::fmt::Debug for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

/// A unit of length: a standard one, which converts, or any other name
/// (`Tag`), which values are simply multiples of.
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
    /// Anything else: the reference's own unit (`u` with none given).
    Tag(Tag),
}

impl Unit {
    pub const STANDARD: [Unit; 8] = [
        Unit::Mm,
        Unit::Cm,
        Unit::M,
        Unit::Km,
        Unit::In,
        Unit::Ft,
        Unit::Yd,
        Unit::Mi,
    ];

    /// No unit given: plain multiples of the reference.
    pub fn plain() -> Unit {
        Unit::Tag(Tag::new("u"))
    }

    pub fn name(&self) -> &str {
        match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::Km => "km",
            Unit::In => "in",
            Unit::Ft => "ft",
            Unit::Yd => "yd",
            Unit::Mi => "mi",
            Unit::Tag(t) => t.as_str(),
        }
    }

    /// Whether it's a named unit of its own, with nothing to convert to.
    pub fn is_tag(self) -> bool {
        matches!(self, Unit::Tag(_))
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
            // Not a length: only ever compared with itself.
            Unit::Tag(_) => 1.0,
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
            t @ Unit::Tag(_) => t,
        }
    }

    /// A standard unit by name, if it is one.
    pub fn standard_name(name: &str) -> Option<Unit> {
        Unit::standard(name)
    }

    /// A unit by name: a standard one, else a name of its own.
    pub fn from_name(name: &str) -> Unit {
        Unit::standard(name).unwrap_or(Unit::Tag(Tag::new(name)))
    }

    /// A standard unit by its name or mark.
    fn standard(s: &str) -> Option<Unit> {
        match s {
            "'" => Some(Unit::Ft),
            "\"" => Some(Unit::In),
            _ => Unit::STANDARD.into_iter().find(|u| u.name() == s),
        }
    }
}

/// A typed length: a value in a unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    pub value: f64,
    pub unit: Unit,
}

fn positive(value: f64, s: &str) -> Result<f64, String> {
    if value > 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{:?}: a length must be more than zero", s.trim()))
    }
}

/// `5 m`, `2.5km`, `2*2.5 m`, `12'6"`, `12ft 6in`: a number (or a little
/// arithmetic) with a standard unit, or several summed, in the first one's
/// unit. Any other name after the number is a unit of its own (`4.2 ly`,
/// `3 light years`); a number alone is in `u`, and nothing at all is 1 u.
pub fn parse_length(s: &str) -> Result<Length, String> {
    let bad = || {
        format!(
            "can't read {:?}: a number, maybe with a unit, like 5 m",
            s.trim()
        )
    };
    let t = s.trim();
    if t.is_empty() {
        return Ok(Length {
            value: 1.0,
            unit: Unit::plain(),
        });
    }
    let is_unit_char = |c: char| c.is_alphabetic() || c == '\'' || c == '"';
    let Some(first) = t.find(is_unit_char) else {
        let value = positive(eval(t).ok_or_else(bad)?, s)?;
        return Ok(Length {
            value,
            unit: Unit::plain(),
        });
    };
    if let Some(l) = standard_length(t)? {
        return Ok(l);
    }
    // A unit of its own: the number, then whatever name follows it.
    let value = positive(eval(&t[..first]).ok_or_else(bad)?, s)?;
    Ok(Length {
        value,
        unit: Unit::Tag(Tag::new(t[first..].trim())),
    })
}

/// `t` in standard units only (`12ft 6in`), if that's what it is; a bad
/// number is an error, a name that isn't a standard unit is `None`.
fn standard_length(t: &str) -> Result<Option<Length>, String> {
    let bad = || format!("can't read {t:?}: a number, maybe with a unit, like 5 m");
    let mut parts: Vec<(f64, Unit)> = Vec::new();
    let mut rest = t;
    while !rest.is_empty() {
        // The number: everything up to the first unit letter or mark.
        let Some(end) = rest.find(|c: char| c.is_alphabetic() || c == '\'' || c == '"') else {
            return Ok(None); // a number with no unit after a standard one
        };
        let value = eval(&rest[..end]).ok_or_else(bad)?;
        let tail = &rest[end..];
        let ulen = if tail.starts_with(['\'', '"']) {
            1
        } else {
            tail.find(|c: char| !c.is_alphabetic())
                .unwrap_or(tail.len())
        };
        let Some(unit) = Unit::standard(&tail[..ulen]) else {
            return Ok(None);
        };
        parts.push((value, unit));
        rest = tail[ulen..].trim_start();
    }
    let Some(&(_, unit)) = parts.first() else {
        return Ok(None);
    };
    let metres: f64 = parts.iter().map(|(v, u)| v * u.metres()).sum();
    let metres = positive(metres, t)?;
    Ok(Some(Length {
        value: metres / unit.metres(),
        unit,
    }))
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

impl From<Scale> for crate::model::MeasureScale {
    fn from(s: Scale) -> Self {
        Self {
            px: s.px,
            real: s.real,
            unit: s.unit.name().to_string(),
            error: s.error,
        }
    }
}

impl From<&crate::model::MeasureScale> for Scale {
    fn from(s: &crate::model::MeasureScale) -> Self {
        Scale {
            px: s.px,
            real: s.real,
            unit: Unit::from_name(&s.unit),
            error: s.error,
        }
    }
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

/// The unit of `system`'s family (metric or imperial; plain stays plain)
/// that suits a length of `metres`: the largest it's at least one of.
pub fn fit(system: Unit, metres: f64) -> Unit {
    const METRIC: [Unit; 4] = [Unit::Km, Unit::M, Unit::Cm, Unit::Mm];
    const IMPERIAL: [Unit; 3] = [Unit::Mi, Unit::Ft, Unit::In];
    let family: &[Unit] = match system {
        t @ Unit::Tag(_) => return t,
        u if u.metric() => &METRIC,
        _ => &IMPERIAL,
    };
    family
        .iter()
        .copied()
        .find(|u| metres.abs() >= u.metres())
        .unwrap_or(family[family.len() - 1])
}

/// A length `px` long, measured from `points` points: `4.20 m ±0.08`, or
/// `352.1 px`. Real lengths take the fitting unit of the display's system.
pub fn length(px: f64, points: usize, scale: Option<&Scale>, d: Display) -> String {
    match (d, scale) {
        (Display::Real(system), Some(s)) => {
            let unit = fit(system, px / s.px_per_metre());
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
        (Display::Real(system), Some(s)) => {
            let unit = fit(system, px / s.px_per_metre());
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
        (Display::Real(system), Some(s)) => {
            // Fitted by its side: 0.09 m² is 900 cm².
            let unit = fit(system, px2.sqrt() / s.px_per_metre());
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
        // Leaked: a test's few names, to compare with literals.
        let name: &'static str = Box::leak(l.unit.name().to_string().into_boxed_str());
        ((l.value * 1e6).round() / 1e6, name)
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
        // No unit: plain units, the reference's own scale; nothing is 1.
        assert_eq!(len("5"), (5.0, "u"));
        assert_eq!(len("2*3"), (6.0, "u"));
        assert_eq!(len(""), (1.0, "u"));
        assert_eq!(len("  "), (1.0, "u"));
        assert_eq!(len("4 u"), (4.0, "u"));
        // Any other name is a unit of its own (see below), so only a
        // missing or non-positive number is wrong.
        for bad in ["m", "0 m", "-2 m", "five m", "five", "0", "0 ly"] {
            assert!(parse_length(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn any_other_unit_is_kept_as_its_name() {
        assert_eq!(len("4.2 ly"), (4.2, "ly"));
        assert_eq!(len("3 light years"), (3.0, "light years"));
        assert_eq!(len("12 bricks"), (12.0, "bricks"));
        assert_eq!(len("5 parsecs"), (5.0, "parsecs"));
        assert_eq!(len("2*3 tiles"), (6.0, "tiles"));
        // The standard ones are still known, and convert.
        assert_eq!(len("5 km"), (5.0, "km"));
        assert!(parse_length("5 km").unwrap().unit.metric());
        let ly = parse_length("1 ly").unwrap().unit;
        assert!(!ly.metric() && ly.other() == ly, "nothing to convert to");
        // A long name is cut, not refused.
        assert_eq!(
            parse_length("1 extraordinarily long unit names here")
                .unwrap()
                .unit
                .name(),
            "extraordinarily long uni"
        );
        // Values read in it, unfitted.
        let s = Scale::parse("100px=1ly").unwrap();
        assert_eq!(s.describe(), "1 ly = 100.0 px");
        assert_eq!(
            length_bare(50.0, Some(&s), Display::Real(s.unit)),
            "0.500 ly"
        );
        assert_eq!(
            length_bare(0.5, Some(&s), Display::Real(s.unit)),
            "0.00500 ly"
        );
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
    fn values_take_the_fitting_unit_of_the_references_system() {
        let at = |scale: &str, px: f64| {
            let s = Scale::parse(scale).unwrap();
            length_bare(px, Some(&s), Display::Real(s.unit))
        };
        // A km reference: smaller things in m, smaller still in cm.
        assert_eq!(at("100px=1km", 250.0), "2.50 km");
        assert_eq!(at("100px=1km", 30.0), "300 m");
        assert_eq!(at("100px=1km", 0.05), "50.0 cm");
        // An m reference: cm and mm below a metre, km above a thousand.
        assert_eq!(at("100px=1m", 30.0), "30.0 cm");
        assert_eq!(at("100px=1m", 0.5), "5.00 mm");
        assert_eq!(at("1px=1m", 2500.0), "2.50 km");
        // Imperial: mi, then ft, then in.
        assert_eq!(at("100px=1mi", 250.0), "2.50 mi");
        assert_eq!(at("100px=1mi", 1.0), "52.8 ft");
        assert_eq!(at("100px=1ft", 50.0), "6.00 in");
        // t: the other system, fitted the same way.
        let s = Scale::parse("100px=1m").unwrap();
        assert_eq!(
            length_bare(30.0, Some(&s), Display::Real(Unit::Ft)),
            "11.8 in"
        );
        // Plain units never change.
        assert_eq!(at("100px=1u", 0.5), "0.00500 u");
        // Areas fit by their side: 0.09 m² is 900 cm².
        let s = Scale::parse("100px=1m").unwrap();
        assert!(area(900.0, 0.0, Some(&s), Display::Real(Unit::M)).starts_with("900 cm²"));
        // The error is in the same unit.
        assert_eq!(
            length(30.0, 2, Some(&s), Display::Real(Unit::M)),
            "30.0 cm ±1.4"
        );
    }

    #[test]
    fn t_switches_system() {
        assert_eq!(Unit::M.other(), Unit::Ft);
        assert_eq!(Unit::Ft.other(), Unit::M);
        assert_eq!(Unit::Km.other().other(), Unit::Km);
    }
}
