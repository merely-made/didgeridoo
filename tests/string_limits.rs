//! The two protections the pattern matcher gives a sandbox: its work is charged to fuel and capped
//! per call, and it has no native recursion, so deep patterns and long subjects cannot overflow
//! the Rust stack. Lua scripts cannot observe either, so they are tested from here.

use piccolo::{Closure, Executor, Fuel, Lua};

const START_FUEL: i32 = i32::MAX;

fn start(lua: &mut Lua, code: &str) -> piccolo::StashedExecutor {
    lua.try_enter(|ctx| {
        let closure = Closure::load(ctx, None, code.as_bytes())?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })
    .unwrap()
}

/// Runs `code` to completion and returns the fuel it used.
fn fuel_used(code: &str) -> i32 {
    let mut lua = Lua::core();
    let exec = start(&mut lua, code);
    let remaining = lua.enter(|ctx| {
        let exec = ctx.fetch(&exec);
        let mut fuel = Fuel::with(START_FUEL);
        while !exec.step(ctx, &mut fuel).unwrap() {
            fuel.refill(START_FUEL, START_FUEL);
        }
        fuel.remaining()
    });
    lua.execute::<()>(&exec).unwrap();
    START_FUEL - remaining
}

fn run(code: &str) {
    let mut lua = Lua::core();
    let exec = start(&mut lua, code);
    lua.execute::<()>(&exec).unwrap();
}

#[test]
fn pattern_work_is_charged_to_fuel() {
    // Both calls fail to find anything. The plain search is the control: it proves the
    // measurement sees a cheap call as cheap.
    let cheap = fuel_used(r#"assert(not string.find(string.rep("a", 3000), "b", 1, true))"#);
    let costly = fuel_used(r#"assert(not string.find(string.rep("a", 3000), ".-b"))"#);
    assert!(cheap < 2_000, "plain search used {cheap} fuel");
    assert!(
        costly > 200_000,
        "backtracking search used only {costly} fuel"
    );
}

#[test]
fn gsub_and_gmatch_work_is_charged_to_fuel() {
    let cheap = fuel_used(r#"local n = select(2, string.gsub(string.rep("a", 3000), "b", "c"))"#);
    let costly = fuel_used(
        r#"local s = string.rep("a", 2000) .. "c"
           assert(select(2, string.gsub(s, "a-c", "")) == 1)
           for _ in string.gmatch(s, "a*b") do end"#,
    );
    assert!(cheap < 5_000, "cheap gsub used {cheap} fuel");
    assert!(
        costly > 100_000,
        "backtracking gsub/gmatch used only {costly} fuel"
    );
}

#[test]
fn one_call_has_a_step_ceiling() {
    // About 5 billion steps if allowed to finish; the ceiling turns it into an error.
    run(r#"local s = string.rep("a", 100000)
           local ok, err = pcall(string.find, s, ".-b")
           assert(not ok)
           assert(err == "pattern match step limit exceeded", err)
           local ok, err = pcall(string.gsub, s, ".-b", "")
           assert(not ok and err == "pattern match step limit exceeded", err)
           local ok, err = pcall(string.match, s, "(.-)b")
           assert(not ok and err == "pattern match step limit exceeded", err)
           -- the ceiling is per call: a normal call afterwards works
           assert(string.find(s, "a", 1, true) == 1)
           assert(string.match(s, "^a+$") == s)"#);
}

#[test]
fn a_hit_ceiling_still_charges_the_fuel_spent() {
    let used = fuel_used(
        r#"local ok = pcall(string.find, string.rep("a", 100000), ".-b")
           assert(not ok)"#,
    );
    assert!(
        used > 1_000_000,
        "a call stopped by the ceiling used only {used} fuel"
    );
}

#[test]
fn pattern_depth_is_capped() {
    run(r#"local function try(n)
             return pcall(string.match, string.rep("a", n), string.rep(".?", n))
           end
           local ok, r = try(200)
           assert(ok and r == string.rep("a", 200))
           local ok, err = try(201)
           assert(not ok and err == "pattern too complex", err)
           local ok, err = try(5000)
           assert(not ok and err == "pattern too complex", err)
           -- lazy items count too
           local ok, err = pcall(string.find, "a", string.rep("a-", 201))
           assert(not ok and err == "pattern too complex", err)
           -- a huge pattern is refused quickly, not matched
           local ok, err = pcall(string.match, string.rep("a", 1000), string.rep(".?", 1000000))
           assert(not ok and err == "pattern too complex", err)"#);
}

#[test]
fn long_subjects_and_patterns_run_on_a_small_native_stack() {
    // A matcher that recursed per byte or per item would overflow this stack long before the
    // subject ends. Release or debug, the thread has 256 KiB.
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            run(r#"local big = string.rep("a", 1000000)
                   assert(#string.match(big, "^(a*)$") == 1000000)
                   assert(#string.match(big, "^(.-)$") == 1000000)
                   assert(string.find(big .. "b", "a-b") == 1)
                   assert(string.find(big, "a+$") == 1)
                   local _, n = string.gsub(big, "a", "b")
                   assert(n == 1000000)
                   local count = 0
                   for _ in string.gmatch(big, "a") do count = count + 1 end
                   assert(count == 1000000)
                   assert(string.find(string.rep("(", 150) .. string.rep(")", 150), "%b()") == 1)
                   local deep = string.rep("a?", 190)
                   assert(#string.match(string.rep("a", 190), deep) == 190)"#)
        })
        .unwrap();
    handle.join().unwrap();
}
