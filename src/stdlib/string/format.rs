//! `string.format`: parsing the format string, and laying out each conversion.

use super::charge_bytes;
use super::numfmt::{
    format_float, format_radix, format_signed, format_unsigned, hex_float, pad_text, Flags, Layout,
};
use crate::async_callback::SequenceReturn;
use crate::{
    async_sequence,
    meta_ops::{self, MetaResult},
    CallbackReturn, Context, Error, Execution, Stack, Value,
};

/// A conversion's `%s` precision at or beyond which a long string skips layout (Lua's own rule:
/// strings of 100 bytes or more with no precision are copied whole).
const WHOLE_STRING_LEN: usize = 100;

/// Most digits in a width or precision.
const MAX_SPEC_DIGITS: usize = 2;

pub enum Piece {
    Lit(Vec<u8>),
    Conv {
        conv: u8,
        layout: Layout,
        /// Index of the argument in the call's stack (the format string is index 0).
        arg: usize,
    },
}

/// The flags each conversion accepts, or `None` if it is not a conversion.
fn allowed_flags(conv: u8) -> Option<&'static [u8]> {
    Some(match conv {
        b'c' | b's' => b"-",
        b'd' | b'i' => b"-+0 ",
        b'u' => b"-0",
        b'o' | b'x' | b'X' => b"-#0",
        b'a' | b'A' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => b"-+ #0",
        b'q' => b"",
        _ => return None,
    })
}

fn bad_conversion(fmt: &[u8], start: usize, end: usize) -> String {
    let end = end.min(fmt.len());
    format!(
        "invalid conversion '{}' to 'format'",
        String::from_utf8_lossy(&fmt[start..end])
    )
}

pub fn parse(fmt: &[u8]) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut lit = Vec::new();
    let mut next_arg = 1;
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' {
            lit.push(c);
            continue;
        }
        if fmt.get(i) == Some(&b'%') {
            lit.push(b'%');
            i += 1;
            continue;
        }
        let start = i - 1;
        let mut flags = Flags::default();
        while let Some(&f) = fmt.get(i) {
            match f {
                b'-' => flags.minus = true,
                b'+' => flags.plus = true,
                b' ' => flags.space = true,
                b'#' => flags.hash = true,
                b'0' => flags.zero = true,
                _ => break,
            }
            i += 1;
        }
        let digits = |i: &mut usize| -> Result<Option<usize>, String> {
            let from = *i;
            while fmt.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            if *i - from > MAX_SPEC_DIGITS {
                return Err(bad_conversion(fmt, start, *i + 1));
            }
            Ok(std::str::from_utf8(&fmt[from..*i])
                .ok()
                .and_then(|d| d.parse().ok()))
        };
        let width = digits(&mut i)?.unwrap_or(0);
        let precision = if fmt.get(i) == Some(&b'.') {
            i += 1;
            Some(digits(&mut i)?.unwrap_or(0))
        } else {
            None
        };
        let Some(&conv) = fmt.get(i) else {
            return Err(bad_conversion(fmt, start, i));
        };
        i += 1;
        let allowed = allowed_flags(conv).ok_or_else(|| bad_conversion(fmt, start, i))?;
        let used = [
            (flags.minus, b'-'),
            (flags.plus, b'+'),
            (flags.space, b' '),
            (flags.hash, b'#'),
            (flags.zero, b'0'),
        ];
        let flags_ok = used.iter().all(|&(on, f)| !on || allowed.contains(&f));
        let no_modifiers = conv == b'q' && (width != 0 || precision.is_some());
        let precision_ok = precision.is_none() || !matches!(conv, b'c' | b'q');
        if !flags_ok || no_modifiers || !precision_ok {
            if conv == b'q' {
                return Err("specifier '%q' cannot have modifiers".into());
            }
            return Err(bad_conversion(fmt, start, i));
        }
        if !lit.is_empty() {
            pieces.push(Piece::Lit(std::mem::take(&mut lit)));
        }
        pieces.push(Piece::Conv {
            conv,
            layout: Layout {
                flags,
                width,
                precision,
            },
            arg: next_arg,
        });
        next_arg += 1;
    }
    if !lit.is_empty() {
        pieces.push(Piece::Lit(lit));
    }
    Ok(pieces)
}

fn error<'gc>(ctx: Context<'gc>, msg: String) -> Error<'gc> {
    Value::String(ctx.intern(msg.as_bytes())).into()
}

fn arg_error<'gc>(ctx: Context<'gc>, arg: usize, msg: &str) -> Error<'gc> {
    error(
        ctx,
        format!("bad argument #{} to 'format' ({msg})", arg + 1),
    )
}

fn integer_arg<'gc>(ctx: Context<'gc>, v: Value<'gc>, arg: usize) -> Result<i64, Error<'gc>> {
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    match v {
        Value::Integer(i) => return Ok(i),
        Value::Number(n) if n.fract() == 0.0 && (-TWO_63..TWO_63).contains(&n) => {
            return Ok(n as i64)
        }
        _ => {}
    }
    if let Some(i) = v.to_integer() {
        if !matches!(v, Value::Number(_)) {
            return Ok(i);
        }
    }
    Err(if v.to_number().is_some() {
        arg_error(ctx, arg, "number has no integer representation")
    } else {
        arg_error(ctx, arg, &format!("number expected, got {}", type_name(v)))
    })
}

fn float_arg<'gc>(ctx: Context<'gc>, v: Value<'gc>, arg: usize) -> Result<f64, Error<'gc>> {
    v.to_number()
        .ok_or_else(|| arg_error(ctx, arg, &format!("number expected, got {}", type_name(v))))
}

fn type_name(v: Value) -> &'static str {
    v.type_name()
}

/// Appends `s` as a Lua string literal.
fn quote(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'"');
    for (i, &c) in s.iter().enumerate() {
        match c {
            b'"' | b'\\' | b'\n' => {
                out.push(b'\\');
                out.push(c);
            }
            c if c.is_ascii_control() => {
                let next_is_digit = s.get(i + 1).is_some_and(u8::is_ascii_digit);
                let esc = if next_is_digit {
                    format!("\\{c:03}")
                } else {
                    format!("\\{c}")
                };
                out.extend_from_slice(esc.as_bytes());
            }
            c => out.push(c),
        }
    }
    out.push(b'"');
}

fn quote_value<'gc>(
    ctx: Context<'gc>,
    out: &mut Vec<u8>,
    v: Value<'gc>,
    arg: usize,
) -> Result<(), Error<'gc>> {
    match v {
        Value::String(s) => quote(out, s.as_bytes()),
        Value::Integer(i) if i == i64::MIN => out.extend_from_slice(b"0x8000000000000000"),
        Value::Integer(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Value::Number(n) if n.is_nan() => out.extend_from_slice(b"(0/0)"),
        Value::Number(n) if n.is_infinite() => {
            out.extend_from_slice(if n > 0.0 { b"1e9999" } else { b"-1e9999" })
        }
        Value::Number(n) => {
            let sign = if n.is_sign_negative() { "-" } else { "" };
            out.extend_from_slice(format!("{sign}{}", hex_float(n.abs(), None, false)).as_bytes());
        }
        Value::Nil | Value::Boolean(_) => out.extend_from_slice(v.display().to_string().as_bytes()),
        _ => return Err(arg_error(ctx, arg, "value has no literal form")),
    }
    Ok(())
}

/// Lays out every piece against the call's arguments. `%s` arguments must already be strings.
pub fn render<'gc>(
    ctx: Context<'gc>,
    pieces: &[Piece],
    stack: &Stack<'gc, '_>,
) -> Result<Vec<u8>, Error<'gc>> {
    let mut out = Vec::new();
    for piece in pieces {
        let (conv, layout, arg) = match piece {
            Piece::Lit(bytes) => {
                out.extend_from_slice(bytes);
                continue;
            }
            Piece::Conv { conv, layout, arg } => (*conv, layout, *arg),
        };
        if arg >= stack.len() {
            return Err(arg_error(ctx, arg, "no value"));
        }
        let v = stack.get(arg);
        match conv {
            b'c' => {
                let byte = integer_arg(ctx, v, arg)? as u8;
                out.extend(pad_text(layout, &[byte]));
            }
            b'd' | b'i' => {
                out.extend(format_signed(layout, integer_arg(ctx, v, arg)?).bytes());
            }
            b'u' => {
                out.extend(format_unsigned(layout, integer_arg(ctx, v, arg)? as u64).bytes());
            }
            b'o' | b'x' | b'X' => {
                let radix = if conv == b'o' { 8 } else { 16 };
                let n = integer_arg(ctx, v, arg)? as u64;
                out.extend(format_radix(layout, n, radix, conv == b'X').bytes());
            }
            b'a' | b'A' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => {
                out.extend(format_float(layout, conv, float_arg(ctx, v, arg)?).bytes());
            }
            b'q' => quote_value(ctx, &mut out, v, arg)?,
            _ => {
                let Value::String(s) = v else {
                    return Err(arg_error(ctx, arg, "string expected"));
                };
                let bytes = s.as_bytes();
                let plain = layout == &Layout::default();
                if plain || (layout.precision.is_none() && bytes.len() >= WHOLE_STRING_LEN) {
                    out.extend_from_slice(bytes);
                } else {
                    if bytes.contains(&0) {
                        return Err(arg_error(ctx, arg, "string contains zeros"));
                    }
                    let keep = layout.precision.map_or(bytes.len(), |p| p.min(bytes.len()));
                    out.extend(pad_text(layout, &bytes[..keep]));
                }
            }
        }
    }
    Ok(out)
}

/// `string.format`. A `%s` argument whose `__tostring` is a Lua function makes this a sequence
/// that calls those first, then lays everything out.
pub fn format_callback<'gc>(
    ctx: Context<'gc>,
    mut exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let Some(fmt) = stack.get(0).into_string(ctx) else {
        let got = type_name(stack.get(0));
        return Err(arg_error(ctx, 0, &format!("string expected, got {got}")));
    };
    let pieces = parse(fmt.as_bytes()).map_err(|m| error(ctx, m))?;
    let mut pending = Vec::new();
    for piece in &pieces {
        if let Piece::Conv {
            conv: b's', arg, ..
        } = piece
        {
            if *arg >= stack.len() {
                return Err(arg_error(ctx, *arg, "no value"));
            }
            match meta_ops::tostring(ctx, stack.get(*arg))? {
                MetaResult::Value(v) => stack[*arg] = v,
                MetaResult::Call(call) => pending.push((*arg, call.function)),
            }
        }
    }
    if pending.is_empty() {
        let out = render(ctx, &pieces, &stack)?;
        charge_bytes(exec.fuel(), out.len());
        stack.replace(ctx, ctx.intern(&out));
        return Ok(CallbackReturn::Return);
    }
    let seq = async_sequence(&ctx, |locals, mut seq| {
        let pending: Vec<_> = pending
            .into_iter()
            .map(|(arg, f)| (arg, locals.stash(&ctx, f)))
            .collect();
        async move {
            for (arg, function) in pending {
                let bottom = seq.enter(|_, _, _, mut stack| {
                    let bottom = stack.len();
                    let original = stack.get(arg);
                    stack.push_back(original);
                    bottom
                });
                seq.call(&function, bottom).await?;
                seq.try_enter(|ctx, _, _, mut stack| {
                    let result = stack.get(bottom);
                    stack.resize(bottom);
                    let text = result
                        .into_string(ctx)
                        .ok_or_else(|| error(ctx, "'__tostring' must return a string".into()))?;
                    stack[arg] = text.into();
                    Ok(())
                })?;
            }
            seq.try_enter(|ctx, _, mut exec, mut stack| {
                let out = render(ctx, &pieces, &stack)?;
                charge_bytes(exec.fuel(), out.len());
                stack.replace(ctx, ctx.intern(&out));
                Ok(())
            })?;
            Ok(SequenceReturn::Return)
        }
    });
    Ok(CallbackReturn::Sequence(seq))
}
