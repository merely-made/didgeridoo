//! The embedder's string ceiling: a script cannot build a string longer than
//! `Context::max_string_bytes`, whichever way it builds it. Each case runs twice, at the limit
//! (the control, which must succeed) and one byte past it (which must fail).

use piccolo::{Closure, Executor, Lua, DEFAULT_MAX_STRING_BYTES};

const LIMIT: usize = 1000;

fn run(limit: Option<usize>, code: &str) -> Result<(), String> {
    let mut lua = Lua::core();
    let exec = lua
        .try_enter(|ctx| {
            if let Some(limit) = limit {
                ctx.set_max_string_bytes(limit);
            }
            let closure = Closure::load(ctx, None, code.as_bytes())?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        })
        .unwrap();
    lua.execute::<()>(&exec).map_err(|e| e.to_string())
}

/// `build` is Lua that builds a string of `n` bytes, with `n` bound as a local.
fn at_and_past(build: &str) {
    let code = |n: usize| format!("local n = {n}\nlocal s = {build}\nassert(#s == n, #s)");
    run(Some(LIMIT), &code(LIMIT)).unwrap_or_else(|e| panic!("at the limit, {build}: {e}"));
    let err = run(Some(LIMIT), &code(LIMIT + 1)).expect_err(build);
    assert!(
        err.contains("too large") || err.contains("too long"),
        "{build}: unexpected error {err}"
    );
}

#[test]
fn rep_is_held_to_the_limit() {
    at_and_past(r#"string.rep("a", n)"#);
    at_and_past(r#"string.rep("ab", n // 3, "c") .. string.rep("z", n - (n // 3) * 3 + 1)"#);
}

#[test]
fn concatenation_is_held_to_the_limit() {
    at_and_past(r#"string.rep("a", n - 1) .. "b""#);
    at_and_past(r#"string.rep("a", n - 3) .. "b" .. "c" .. "d""#);
}

#[test]
fn table_concat_is_held_to_the_limit() {
    at_and_past(r#"table.concat({string.rep("a", n - 4), "b", "c"}, ",")"#);
    at_and_past(r#"table.concat({string.rep("a", n - 2), "b", "c"})"#);
}

#[test]
fn format_is_held_to_the_limit() {
    at_and_past(r#"string.format("%s%s", string.rep("a", n - 1), "b")"#);
    at_and_past(r#"string.format("x%sy", string.rep("a", n - 2))"#);
}

#[test]
fn gsub_is_held_to_the_limit() {
    // Each of n // 2 matches is replaced by two bytes; the tail pads to n.
    at_and_past(r#"(string.gsub(string.rep("a", n // 2), "a", "bb")) .. string.rep("z", n % 2)"#);
    at_and_past(
        r#"(string.gsub(string.rep("a", n // 2), "a", function() return "bb" end))
           .. string.rep("z", n % 2)"#,
    );
    at_and_past(
        r#"(string.gsub(string.rep("a", n // 2), "a", {a = "bb"})) .. string.rep("z", n % 2)"#,
    );
}

#[test]
fn gsub_alone_is_refused_past_the_limit() {
    // The cases above end in a concatenation; this one can only be refused by gsub.
    let code = |n: usize| format!(r#"local s = string.gsub(string.rep("a", {n}), "a", "bb")"#);
    run(Some(LIMIT), &code(LIMIT / 2)).unwrap();
    let err = run(Some(LIMIT), &code(LIMIT / 2 + 1)).unwrap_err();
    assert!(err.contains("too large"), "{err}");
    let by_function = format!(
        r#"local s = string.gsub(string.rep("a", {}), "a", function() return "bb" end)"#,
        LIMIT / 2 + 1
    );
    let err = run(Some(LIMIT), &by_function).unwrap_err();
    assert!(err.contains("too large"), "{err}");
}

#[test]
fn the_default_is_unchanged_and_readable() {
    let mut lua = Lua::core();
    lua.enter(|ctx| assert_eq!(ctx.max_string_bytes(), DEFAULT_MAX_STRING_BYTES));
    // Without a limit set, a string well past LIMIT builds.
    run(None, r#"assert(#string.rep("a", 100000) == 100000)"#).unwrap();
}

#[test]
fn a_refusal_is_catchable_and_the_state_goes_on() {
    run(
        Some(LIMIT),
        r#"local ok, err = pcall(string.rep, "a", 5000)
           assert(not ok and tostring(err):find("too large"), tostring(err))
           assert(#string.rep("a", 10) == 10)"#,
    )
    .unwrap();
}
