use piccolo::{Closure, Executor, Lua, Table, Value};

fn run(lua: &mut Lua, code: &str) -> String {
    let exec = lua
        .try_enter(|ctx| {
            let c = Closure::load(ctx, Some("t"), code.as_bytes())?;
            Ok(ctx.stash(Executor::start(ctx, c.into(), ())))
        })
        .unwrap();
    lua.execute::<String>(&exec).unwrap()
}

fn is_lua_closure(lua: &mut Lua, name: &'static str) -> bool {
    lua.enter(|ctx| {
        let t: Table = ctx.get_global("table").unwrap();
        matches!(
            t.get_raw(piccolo::IntoValue::into_value(name, ctx)),
            Value::Function(piccolo::Function::Closure(_))
        )
    })
}

#[test]
fn untouched_state_keeps_placeholders() {
    let mut lua = Lua::core();
    assert!(!is_lua_closure(&mut lua, "sort"));
    assert!(!is_lua_closure(&mut lua, "move"));
    let r = run(
        &mut lua,
        r#"local n = 0
        for k in pairs(table) do n = n + 1 end
        return n .. type(table.sort) .. type(rawget(table, "move"))"#,
    );
    assert_eq!(r, "7functionfunction");
    // Reading the names does not compile them.
    assert!(!is_lua_closure(&mut lua, "sort"));
}

#[test]
fn sort_and_move_work_and_replace_placeholders() {
    let mut lua = Lua::core();
    let r = run(
        &mut lua,
        r#"local stashed = table.sort
        local t = {3, 1, 2}
        table.sort(t)
        local u = {3, 1, 2}
        stashed(u, function(a, b) return a > b end)
        local m = table.move({1, 2, 3}, 1, 3, 2)
        return table.concat(t, ",") .. "|" .. table.concat(u, ",") .. "|" .. table.concat(m, ",")"#,
    );
    assert_eq!(r, "1,2,3|3,2,1|1,1,2,3");
    assert!(is_lua_closure(&mut lua, "sort"));
    assert!(is_lua_closure(&mut lua, "move"));
}

#[test]
fn comparator_error_propagates_on_first_call() {
    let mut lua = Lua::core();
    let r = run(
        &mut lua,
        r#"local ok, e = pcall(table.sort, {3, 1, 2}, function() error("boom") end)
        local ok2, e2 = pcall(table.move, 1, 2, 3, 4)
        return tostring(ok) .. tostring(e):match("boom") .. tostring(ok2) .. tostring(e2):match("must be tables")"#,
    );
    assert_eq!(r, "falseboomfalsemust be tables");
}

#[test]
fn user_replacement_is_not_overwritten() {
    let mut lua = Lua::core();
    let r = run(
        &mut lua,
        r#"local orig = table.sort
        local mine = function() return "mine" end
        table.sort = mine
        orig({2, 1})
        return table.sort == mine and "kept" or "clobbered""#,
    );
    assert_eq!(r, "kept");
}
