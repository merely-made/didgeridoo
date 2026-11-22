//! The `utf8` library, so far `utf8.len` alone.

use crate::{Callback, CallbackReturn, Context, IntoValue, String, Table, Value};

/// Bytes scanned per unit of fuel.
const BYTES_PER_FUEL: usize = 32;

/// The longest encoding of one character.
const MAX_CHAR_BYTES: usize = 4;

/// A byte position as Lua gives it (1-based, negative from the end) as a 0-based offset, or
/// `None` when it falls before the start.
fn offset(position: i64, len: usize) -> Option<usize> {
    if position > 0 {
        usize::try_from(position - 1).ok()
    } else {
        let back = usize::try_from(position.unsigned_abs()).ok()?;
        len.checked_sub(back)
    }
}

/// The byte length of the character that starts `bytes`, or `None` if it is not valid UTF-8.
fn char_len(bytes: &[u8]) -> Option<usize> {
    let head = &bytes[..bytes.len().min(MAX_CHAR_BYTES)];
    let valid = match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(error) => std::str::from_utf8(&head[..error.valid_up_to()]).ok()?,
    };
    valid.chars().next().map(char::len_utf8)
}

pub fn load_utf8<'gc>(ctx: Context<'gc>) {
    let utf8 = Table::new(&ctx);

    utf8.set_field(
        ctx,
        "len",
        Callback::from_fn(&ctx, |ctx, mut exec, mut stack| {
            let (s, i, j) = stack.consume::<(String, Option<i64>, Option<i64>)>(ctx)?;
            let bytes = s.as_bytes();
            let len = bytes.len();
            // `i` may sit one past the end, which counts nothing; 0 is no position.
            let start = Some(i.unwrap_or(1))
                .filter(|&i| i != 0)
                .and_then(|i| offset(i, len))
                .filter(|&start| start <= len)
                .ok_or_else(|| {
                    "bad argument #2 to 'len' (initial position out of bounds)".into_value(ctx)
                })?;
            // `j` is the last byte a character may start at. A negative `j` counts from the
            // end and, as in Lua, one before the start means "none".
            let last = match j.unwrap_or(-1) {
                j if j > 0 => Some(offset(j, len).filter(|&last| last < len).ok_or_else(|| {
                    "bad argument #3 to 'len' (final position out of bounds)".into_value(ctx)
                })?),
                j => offset(j, len).filter(|_| j < 0),
            };
            let scanned = last.map_or(0, |last| last.saturating_sub(start).saturating_add(1));
            let units = (scanned / BYTES_PER_FUEL + 1).min(i32::MAX as usize);
            exec.fuel().consume(units as i32);

            let mut at = start;
            let mut count: i64 = 0;
            while last.is_some_and(|last| at <= last) {
                match char_len(&bytes[at..]) {
                    Some(width) => {
                        at += width;
                        count += 1;
                    }
                    None => {
                        // Lua reports the 1-based position of the first invalid byte.
                        stack.replace(ctx, (Value::Nil, at as i64 + 1));
                        return Ok(CallbackReturn::Return);
                    }
                }
            }
            stack.replace(ctx, count);
            Ok(CallbackReturn::Return)
        }),
    );

    ctx.set_global("utf8", utf8);
}
