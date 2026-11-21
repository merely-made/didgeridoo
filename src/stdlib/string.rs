use crate::{Callback, CallbackReturn, Context, FromValue, Fuel, IntoValue, String, Table, Value};

mod find;
mod format;
mod numfmt;
mod pattern;

/// Bytes of string built or copied per unit of fuel.
const BYTES_PER_FUEL: usize = 32;

/// Longest string `rep` will build, so one call cannot exhaust the host's memory.
const MAX_BUILT_STRING_BYTES: usize = 1 << 30;

/// Charges the fuel for `bytes` of string work, at least one unit.
fn charge_bytes(fuel: &mut Fuel, bytes: usize) {
    let units = (bytes / BYTES_PER_FUEL + 1).min(i32::MAX as usize);
    fuel.consume(units as i32);
}

pub fn load_string<'gc>(ctx: Context<'gc>) {
    let string = Table::new(&ctx);

    string.set_field(
        ctx,
        "len",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let len = string.len();
            stack.replace(ctx, len);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "byte",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (string, i, j) = stack.consume::<(String, Option<i64>, Option<i64>)>(ctx)?;
            let i = i.unwrap_or(1);
            let substr = sub(string.as_bytes(), i, j.or(Some(i)))?;
            stack.extend(substr.iter().map(|b| Value::Integer(i64::from(*b))));
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "char",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = ctx.intern(
                &stack
                    .into_iter()
                    .map(|c| u8::from_value(ctx, c))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            stack.replace(ctx, string);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "sub",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (string, i, j) = stack.consume::<(String, i64, Option<i64>)>(ctx)?;
            let substr = ctx.intern(sub(string.as_bytes(), i, j)?);
            stack.replace(ctx, substr);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "lower",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let lowered = ctx.intern(
                &string
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_lowercase)
                    .collect::<Vec<_>>(),
            );
            stack.replace(ctx, lowered);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "reverse",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let reversed = ctx.intern(&string.as_bytes().iter().copied().rev().collect::<Vec<_>>());
            stack.replace(ctx, reversed);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "upper",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let uppered = ctx.intern(
                &string
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_uppercase)
                    .collect::<Vec<_>>(),
            );
            stack.replace(ctx, uppered);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(ctx, "find", Callback::from_fn(&ctx, find::find));
    string.set_field(ctx, "match", Callback::from_fn(&ctx, find::match_));
    string.set_field(ctx, "gmatch", Callback::from_fn(&ctx, find::gmatch));
    string.set_field(ctx, "gsub", Callback::from_fn(&ctx, find::gsub));
    string.set_field(
        ctx,
        "format",
        Callback::from_fn(&ctx, format::format_callback),
    );

    string.set_field(
        ctx,
        "rep",
        Callback::from_fn(&ctx, |ctx, mut exec, mut stack| {
            let (string, n, sep) = stack.consume::<(String, i64, Option<String>)>(ctx)?;
            let sep = sep.map_or(&[][..], |s| s.as_bytes());
            let unit = string.as_bytes();
            if n <= 0 {
                stack.replace(ctx, ctx.intern(b""));
                return Ok(CallbackReturn::Return);
            }
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            let total = unit
                .len()
                .checked_add(sep.len())
                .and_then(|piece| piece.checked_mul(n))
                .map(|t| t - sep.len())
                .filter(|&t| t <= MAX_BUILT_STRING_BYTES)
                .ok_or_else(|| "resulting string too large".into_value(ctx))?;
            charge_bytes(exec.fuel(), total);
            let mut out = Vec::with_capacity(total);
            for i in 0..n {
                if i > 0 {
                    out.extend_from_slice(sep);
                }
                out.extend_from_slice(unit);
            }
            stack.replace(ctx, ctx.intern(&out));
            Ok(CallbackReturn::Return)
        }),
    );

    crate::meta_ops::string_metatable(ctx).set_field(ctx, "__index", string);
    ctx.set_global("string", string);
}

fn sub(string: &[u8], i: i64, j: Option<i64>) -> Result<&[u8], std::num::TryFromIntError> {
    let i = match i {
        i if i > 0 => i.saturating_sub(1).try_into()?,
        0 => 0,
        i => string.len().saturating_sub(i.unsigned_abs().try_into()?),
    };
    let j = if let Some(j) = j {
        if j >= 0 {
            j.try_into()?
        } else {
            let j: usize = j.unsigned_abs().try_into()?;
            string.len().saturating_sub(j.saturating_sub(1))
        }
    } else {
        string.len()
    }
    .clamp(0, string.len());

    Ok(if i >= j || i >= string.len() {
        &[]
    } else {
        &string[i..j]
    })
}
