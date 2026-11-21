//! C `printf`-style number layout for `string.format`: flags, width, precision.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub minus: bool,
    pub plus: bool,
    pub space: bool,
    pub hash: bool,
    pub zero: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Layout {
    pub flags: Flags,
    pub width: usize,
    pub precision: Option<usize>,
}

/// Pads `body` to the layout's width, with spaces or (for `zero_ok` numbers) zeros after `lead`.
fn pad(layout: &Layout, lead: &str, body: &str, zero_ok: bool) -> String {
    let len = lead.len() + body.len();
    let fill = layout.width.saturating_sub(len);
    if layout.flags.minus {
        format!("{lead}{body}{}", " ".repeat(fill))
    } else if layout.flags.zero && zero_ok {
        format!("{lead}{}{body}", "0".repeat(fill))
    } else {
        format!("{}{lead}{body}", " ".repeat(fill))
    }
}

/// Pads `text` with spaces only (for `%c` and `%s`).
pub fn pad_text(layout: &Layout, text: &[u8]) -> Vec<u8> {
    let fill = layout.width.saturating_sub(text.len());
    let mut out = Vec::with_capacity(text.len() + fill);
    if !layout.flags.minus {
        out.resize(fill, b' ');
    }
    out.extend_from_slice(text);
    if layout.flags.minus {
        out.resize(text.len() + fill, b' ');
    }
    out
}

fn sign_str(negative: bool, flags: &Flags) -> &'static str {
    if negative {
        "-"
    } else if flags.plus {
        "+"
    } else if flags.space {
        " "
    } else {
        ""
    }
}

/// Lays out an integer's digits: precision is a minimum digit count.
fn int_body(layout: &Layout, digits: String, is_zero: bool) -> String {
    match layout.precision {
        Some(0) if is_zero => String::new(),
        Some(p) if digits.len() < p => format!("{}{digits}", "0".repeat(p - digits.len())),
        _ => digits,
    }
}

pub fn format_signed(layout: &Layout, v: i64) -> String {
    let body = int_body(layout, v.unsigned_abs().to_string(), v == 0);
    let zero_ok = layout.precision.is_none();
    pad(layout, sign_str(v < 0, &layout.flags), &body, zero_ok)
}

pub fn format_unsigned(layout: &Layout, v: u64) -> String {
    let body = int_body(layout, v.to_string(), v == 0);
    pad(layout, "", &body, layout.precision.is_none())
}

/// `radix` is 8 or 16; `upper` picks `A-F` and `0X`.
pub fn format_radix(layout: &Layout, v: u64, radix: u32, upper: bool) -> String {
    let digits = match (radix, upper) {
        (8, _) => format!("{v:o}"),
        (_, false) => format!("{v:x}"),
        (_, true) => format!("{v:X}"),
    };
    let mut body = int_body(layout, digits, v == 0);
    let mut lead = "";
    if layout.flags.hash {
        if radix == 8 {
            if !body.starts_with('0') {
                body.insert(0, '0');
            }
        } else if v != 0 {
            lead = if upper { "0X" } else { "0x" };
        }
    }
    pad(layout, lead, &body, layout.precision.is_none())
}

/// Formats `x` for conversion `conv` (one of `eEfFgGaA`).
pub fn format_float(layout: &Layout, conv: u8, x: f64) -> String {
    let upper = conv.is_ascii_uppercase();
    let negative = x.is_sign_negative();
    let lead = sign_str(negative, &layout.flags);
    let abs = x.abs();
    if !abs.is_finite() {
        let text = if abs.is_nan() { "nan" } else { "inf" };
        let text = if upper {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        return pad(layout, lead, &text, false);
    }
    let hash = layout.flags.hash;
    let body = match conv.to_ascii_lowercase() {
        b'f' => fixed(abs, layout.precision.unwrap_or(6), hash),
        b'e' => exponent(abs, layout.precision.unwrap_or(6), hash),
        b'g' => general(abs, layout.precision, hash),
        _ => hex_float(abs, layout.precision, hash),
    };
    let body = if upper { body.to_uppercase() } else { body };
    pad(layout, lead, &body, true)
}

fn fixed(abs: f64, prec: usize, hash: bool) -> String {
    let mut s = format!("{abs:.prec$}");
    if hash && prec == 0 {
        s.push('.');
    }
    s
}

/// `d.ddde+XX` with a two-digit-minimum exponent.
fn exponent(abs: f64, prec: usize, hash: bool) -> String {
    let raw = format!("{abs:.prec$e}");
    let (mantissa, exp) = raw.split_once('e').unwrap_or((&raw, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let dot = if hash && prec == 0 { "." } else { "" };
    format!(
        "{mantissa}{dot}e{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

fn general(abs: f64, precision: Option<usize>, hash: bool) -> String {
    let p = match precision {
        Some(0) => 1,
        Some(p) => p,
        None => 6,
    };
    let x = if abs == 0.0 {
        0
    } else {
        let raw = format!("{abs:.prec$e}", prec = p - 1);
        raw.split_once('e')
            .and_then(|(_, e)| e.parse::<i32>().ok())
            .unwrap_or(0)
    };
    let (text, has_exp) = if x < -4 || x >= p as i32 {
        (exponent(abs, p - 1, hash), true)
    } else {
        (fixed(abs, (p as i32 - 1 - x) as usize, hash), false)
    };
    if hash {
        return text;
    }
    let (mantissa, tail) = match (has_exp, text.find('e')) {
        (true, Some(i)) => text.split_at(i),
        _ => (text.as_str(), ""),
    };
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    format!("{mantissa}{tail}")
}

/// `0x1.8p+3` style, for finite non-negative `abs`.
pub fn hex_float(abs: f64, precision: Option<usize>, hash: bool) -> String {
    const FRAC_DIGITS: usize = 13;
    let bits = abs.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let mut frac = bits & ((1u64 << 52) - 1);
    let (mut lead, exp) = match (exp_bits, frac) {
        (0, 0) => (0u64, 0),
        (0, _) => (0, -1022),
        _ => (1, exp_bits - 1023),
    };
    let digits = match precision {
        None => format!("{frac:013x}").trim_end_matches('0').to_string(),
        Some(n) if n >= FRAC_DIGITS => format!("{frac:013x}{}", "0".repeat(n - FRAC_DIGITS)),
        Some(n) => {
            let shift = ((FRAC_DIGITS - n) * 4) as u32;
            let rem = frac & ((1u64 << shift) - 1);
            frac >>= shift;
            let half = 1u64 << (shift - 1);
            let odd = if n == 0 { lead & 1 } else { frac & 1 } == 1;
            if rem > half || (rem == half && odd) {
                frac += 1;
                if n == 0 || frac >> (n * 4) != 0 {
                    lead += 1;
                    frac = 0;
                }
            }
            if n == 0 {
                String::new()
            } else {
                format!("{frac:0n$x}")
            }
        }
    };
    let dot = if digits.is_empty() && !hash { "" } else { "." };
    format!(
        "0x{lead}{dot}{digits}p{}{}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}
