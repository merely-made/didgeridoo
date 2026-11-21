//! `string.find`, `match`, `gmatch` and `gsub`, built on the matcher in `pattern.rs`.

use std::cell::Cell;

use gc_arena::Collect;

use super::pattern::{find_plain, is_plain, CaptureValue, Matcher, PatternError};
use crate::async_callback::SequenceReturn;
use crate::{
    async_sequence,
    meta_ops::{self, MetaResult},
    Callback, CallbackReturn, Context, Error, Execution, Fuel, Stack, StashedFunction,
    StashedTable, String, Value,
};

/// Matcher steps per unit of fuel.
const STEPS_PER_FUEL: u64 = 16;

fn charge_steps(fuel: &mut Fuel, steps: u64) {
    let units = (steps / STEPS_PER_FUEL + 1).min(i32::MAX as u64);
    fuel.consume(units as i32);
}

fn error<'gc>(ctx: Context<'gc>, msg: &str) -> Error<'gc> {
    Value::String(ctx.intern(msg.as_bytes())).into()
}

fn pat_err<'gc>(ctx: Context<'gc>, e: PatternError) -> Error<'gc> {
    error(ctx, &e.0)
}

/// 0-based start for a 1-based `init` that may be negative (counting from the end).
fn start_index(init: i64, len: usize) -> usize {
    if init > 0 {
        usize::try_from(init - 1).unwrap_or(usize::MAX)
    } else if init == 0 {
        0
    } else {
        let back = init.unsigned_abs();
        if back > len as u64 {
            0
        } else {
            len - back as usize
        }
    }
}

fn capture_value<'gc>(ctx: Context<'gc>, src: &[u8], c: CaptureValue) -> Value<'gc> {
    match c {
        CaptureValue::Span(a, b) => Value::String(ctx.intern(&src[a..b])),
        CaptureValue::Position(p) => Value::Integer(p as i64),
    }
}

/// The captures of the last match `s..e`; with none, the whole match if `whole` is set.
fn captures<'gc>(
    ctx: Context<'gc>,
    m: &Matcher,
    src: &[u8],
    (s, e): (usize, usize),
    whole: bool,
) -> Result<Vec<Value<'gc>>, Error<'gc>> {
    let n = if m.level() == 0 && whole {
        1
    } else {
        m.level()
    };
    (0..n)
        .map(|i| {
            m.capture(i, s, e)
                .map(|c| capture_value(ctx, src, c))
                .map_err(|e| pat_err(ctx, e))
        })
        .collect()
}

pub fn find<'gc>(
    ctx: Context<'gc>,
    exec: Execution<'gc, '_>,
    stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    find_or_match(ctx, exec, stack, true)
}

pub fn match_<'gc>(
    ctx: Context<'gc>,
    exec: Execution<'gc, '_>,
    stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    find_or_match(ctx, exec, stack, false)
}

fn find_or_match<'gc>(
    ctx: Context<'gc>,
    mut exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
    is_find: bool,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let (s, p, init, plain) = stack.consume::<(String, String, Option<i64>, Value)>(ctx)?;
    let (src, pat) = (s.as_bytes(), p.as_bytes());
    let start = start_index(init.unwrap_or(1), src.len());
    stack.clear();
    if start > src.len() {
        stack.push_back(Value::Nil);
        return Ok(CallbackReturn::Return);
    }
    let mut m = Matcher::new(src, pat);
    let result = run_find(
        ctx,
        &mut m,
        src,
        pat,
        start,
        is_find && (plain.to_bool() || is_plain(pat)),
        is_find,
    );
    charge_steps(exec.fuel(), m.steps());
    match result? {
        Some(values) => stack.extend(values),
        None => stack.push_back(Value::Nil),
    }
    Ok(CallbackReturn::Return)
}

fn run_find<'gc>(
    ctx: Context<'gc>,
    m: &mut Matcher,
    src: &[u8],
    pat: &[u8],
    start: usize,
    plain: bool,
    is_find: bool,
) -> Result<Option<Vec<Value<'gc>>>, Error<'gc>> {
    if plain {
        let found = find_plain(m, src, pat, start).map_err(|e| pat_err(ctx, e))?;
        return Ok(found.map(|i| vec![(i as i64 + 1).into(), ((i + pat.len()) as i64).into()]));
    }
    let anchored = pat.first() == Some(&b'^');
    let hit = m
        .search(start, anchored as usize, anchored)
        .map_err(|e| pat_err(ctx, e))?;
    let Some((s, e)) = hit else {
        return Ok(None);
    };
    let caps = captures(ctx, m, src, (s, e), !is_find)?;
    if !is_find {
        return Ok(Some(caps));
    }
    let mut values = vec![Value::Integer(s as i64 + 1), Value::Integer(e as i64)];
    values.extend(caps);
    Ok(Some(values))
}

#[derive(Collect)]
#[collect(no_drop)]
struct GmatchState<'gc> {
    src: String<'gc>,
    pat: String<'gc>,
    #[collect(require_static)]
    pos: Cell<usize>,
    #[collect(require_static)]
    last_match: Cell<Option<usize>>,
}

pub fn gmatch<'gc>(
    ctx: Context<'gc>,
    _exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let (src, pat, init) = stack.consume::<(String, String, Option<i64>)>(ctx)?;
    let start = start_index(init.unwrap_or(1), src.as_bytes().len()).min(src.as_bytes().len() + 1);
    let state = GmatchState {
        src,
        pat,
        pos: Cell::new(start),
        last_match: Cell::new(None),
    };
    let iter = Callback::from_fn_with(&ctx, state, |st, ctx, mut exec, mut stack| {
        stack.clear();
        let (src, pat) = (st.src.as_bytes(), st.pat.as_bytes());
        let mut m = Matcher::new(src, pat);
        let mut s = st.pos.get();
        let mut found = None;
        let mut failure = None;
        while s <= src.len() {
            match m.match_at(s, 0) {
                Ok(Some(e)) if Some(e) != st.last_match.get() => {
                    found = Some((s, e));
                    break;
                }
                Ok(_) => s += 1,
                Err(err) => {
                    failure = Some(err);
                    break;
                }
            }
        }
        charge_steps(exec.fuel(), m.steps());
        if let Some(err) = failure {
            return Err(pat_err(ctx, err));
        }
        match found {
            Some((s, e)) => {
                st.pos.set(e);
                st.last_match.set(Some(e));
                stack.extend(captures(ctx, &m, src, (s, e), true)?);
            }
            None => {
                st.pos.set(src.len() + 1);
                stack.push_back(Value::Nil);
            }
        }
        Ok(CallbackReturn::Return)
    });
    stack.replace(ctx, iter);
    Ok(CallbackReturn::Return)
}

/// What `gsub` replaces matches with.
enum Replacement<'gc> {
    Text(&'gc [u8]),
    Table(crate::Table<'gc>),
    Function(crate::Function<'gc>),
}

pub fn gsub<'gc>(
    ctx: Context<'gc>,
    mut exec: Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let (s, p, repl, max_n) = stack.consume::<(String, String, Value, Option<i64>)>(ctx)?;
    let (src, pat) = (s.as_bytes(), p.as_bytes());
    let max_n = max_n.map_or(src.len() as u64 + 1, |n| n.max(0) as u64);
    let anchored = pat.first() == Some(&b'^');
    let repl = match repl {
        Value::Table(t) => Replacement::Table(t),
        Value::Function(f) => Replacement::Function(f),
        v => match v.into_string(ctx) {
            Some(text) => Replacement::Text(text.as_bytes()),
            None => {
                let msg = format!(
                    "bad argument #3 to 'gsub' (string/function/table expected, got {})",
                    v.type_name()
                );
                return Err(error(ctx, &msg));
            }
        },
    };
    let text = match repl {
        Replacement::Text(text) => text,
        other => return gsub_with_lua(ctx, s, p, other, max_n, anchored),
    };
    let mut m = Matcher::new(src, pat);
    let mut out = Vec::with_capacity(src.len());
    let mut count = 0u64;
    let mut at = 0;
    let mut last = None;
    let run = (|| -> Result<(), Error<'gc>> {
        while count < max_n {
            let hit = m
                .match_at(at, anchored as usize)
                .map_err(|e| pat_err(ctx, e))?;
            match hit {
                Some(e) if Some(e) != last => {
                    count += 1;
                    add_text(ctx, &mut out, text, &m, src, (at, e))?;
                    at = e;
                    last = Some(e);
                }
                _ if at < src.len() => {
                    out.push(src[at]);
                    at += 1;
                }
                _ => break,
            }
            if anchored {
                break;
            }
        }
        Ok(())
    })();
    charge_steps(exec.fuel(), m.steps());
    super::charge_bytes(exec.fuel(), out.len());
    run?;
    out.extend_from_slice(&src[at..]);
    stack.replace(ctx, (ctx.intern(&out), count as i64));
    Ok(CallbackReturn::Return)
}

/// Appends the replacement string for one match, expanding `%0`-`%9` and `%%`.
fn add_text<'gc>(
    ctx: Context<'gc>,
    out: &mut Vec<u8>,
    text: &[u8],
    m: &Matcher,
    src: &[u8],
    (s, e): (usize, usize),
) -> Result<(), Error<'gc>> {
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let Some(&d) = text.get(i) else {
            return Err(error(ctx, "invalid use of '%' in replacement string"));
        };
        i += 1;
        match d {
            b'%' => out.push(b'%'),
            b'0' => out.extend_from_slice(&src[s..e]),
            b'1'..=b'9' => {
                let index = (d - b'1') as usize;
                let cap = m.capture(index, s, e).map_err(|err| {
                    let suffix = if err.0.starts_with("invalid capture index") {
                        " in replacement string"
                    } else {
                        ""
                    };
                    error(ctx, &format!("{}{suffix}", err.0))
                })?;
                match cap {
                    CaptureValue::Span(a, b) => out.extend_from_slice(&src[a..b]),
                    CaptureValue::Position(p) => out.extend_from_slice(p.to_string().as_bytes()),
                }
            }
            _ => return Err(error(ctx, "invalid use of '%' in replacement string")),
        }
    }
    Ok(())
}

/// The bytes a table or function result stands for, or `None` to keep the match as it was.
fn replacement_bytes<'gc>(ctx: Context<'gc>, v: Value<'gc>) -> Result<Option<Vec<u8>>, Error<'gc>> {
    if !v.to_bool() {
        return Ok(None);
    }
    match v.into_string(ctx) {
        Some(s) => Ok(Some(s.as_bytes().to_vec())),
        None => Err(error(
            ctx,
            &format!("invalid replacement value (a {})", v.type_name()),
        )),
    }
}

/// `gsub` with a table or function: each match may call back into Lua, so this is a sequence.
fn gsub_with_lua<'gc>(
    ctx: Context<'gc>,
    s: String<'gc>,
    p: String<'gc>,
    repl: Replacement<'gc>,
    max_n: u64,
    anchored: bool,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let seq = async_sequence(&ctx, |locals, mut seq| {
        let src = s.as_bytes().to_vec();
        let pat = p.as_bytes().to_vec();
        let (table, function): (Option<StashedTable>, Option<StashedFunction>) = match repl {
            Replacement::Table(t) => (Some(locals.stash(&ctx, t)), None),
            Replacement::Function(f) => (None, Some(locals.stash(&ctx, f))),
            Replacement::Text(_) => (None, None),
        };
        async move {
            let mut m = Matcher::new(&src, &pat);
            let mut out = Vec::with_capacity(src.len());
            let (mut count, mut at, mut last) = (0u64, 0usize, None);
            while count < max_n {
                let hit = seq.try_enter(|ctx, _, _, _| {
                    m.match_at(at, anchored as usize)
                        .map_err(|e| pat_err(ctx, e))
                })?;
                match hit {
                    Some(e) if Some(e) != last => {
                        count += 1;
                        let call = seq.try_enter(|ctx, locals, _, mut stack| {
                            stack.clear();
                            let caps = captures(ctx, &m, &src, (at, e), true)?;
                            if let Some(f) = &function {
                                stack.extend(caps);
                                return Ok(Some(locals.stash(&ctx, locals.fetch(f))));
                            }
                            let table = locals.fetch(table.as_ref().unwrap());
                            match meta_ops::index(ctx, Value::Table(table), caps[0])? {
                                MetaResult::Value(v) => {
                                    stack.push_back(v);
                                    Ok(None)
                                }
                                MetaResult::Call(call) => {
                                    stack.extend(call.args);
                                    Ok(Some(locals.stash(&ctx, call.function)))
                                }
                            }
                        })?;
                        if let Some(f) = call {
                            seq.call(&f, 0).await?;
                        }
                        let new = seq.try_enter(|ctx, _, mut exec, stack| {
                            charge_steps(exec.fuel(), m.take_steps());
                            replacement_bytes(ctx, stack.get(0))
                        })?;
                        match new {
                            Some(bytes) => out.extend_from_slice(&bytes),
                            None => out.extend_from_slice(&src[at..e]),
                        }
                        at = e;
                        last = Some(e);
                    }
                    _ if at < src.len() => {
                        out.push(src[at]);
                        at += 1;
                    }
                    _ => break,
                }
                if anchored {
                    break;
                }
                if !seq.enter(|_, _, mut exec, _| exec.fuel().should_continue()) {
                    seq.pending().await;
                }
            }
            out.extend_from_slice(&src[at.min(src.len())..]);
            seq.enter(|ctx, _, mut exec, mut stack| {
                charge_steps(exec.fuel(), m.take_steps());
                super::charge_bytes(exec.fuel(), out.len());
                stack.replace(ctx, (ctx.intern(&out), count as i64));
            });
            Ok(SequenceReturn::Return)
        }
    });
    Ok(CallbackReturn::Sequence(seq))
}
