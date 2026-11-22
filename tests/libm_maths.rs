//! Float results are libm's, bit for bit: `^` and every `math` function that the platform's C
//! library would round in its own way. The platform's results differ from libm's in up to a
//! tenth of these samples on some machines, so the comparison is exact on purpose.

use piccolo::{Closure, Executor, Lua, Table, Value};

const SAMPLES: i64 = 2000;

/// The two inputs of sample `i`, worked out the same way in Lua and in Rust: `x` spans a few
/// hundred either side of zero, `y` a few units.
const INPUTS: &str = "local x = i * 0.37 - 300.0 local y = i * 0.011 - 9.0";

fn inputs(i: i64) -> (f64, f64) {
    (i as f64 * 0.37 - 300.0, i as f64 * 0.011 - 9.0)
}

fn lua_results(expr: &str) -> Vec<f64> {
    let code =
        format!("local out = {{}} for i = 1, {SAMPLES} do {INPUTS} out[i] = {expr} end return out");
    let mut lua = Lua::core();
    let exec = lua
        .try_enter(|ctx| {
            let closure = Closure::load(ctx, None, code.as_bytes())?;
            Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
        })
        .unwrap();
    lua.finish(&exec).unwrap();
    lua.try_enter(|ctx| {
        let table = ctx.fetch(&exec).take_result::<Table>(ctx)??;
        Ok((1..=SAMPLES)
            .map(|i| match table.get_value(ctx, i) {
                Value::Number(n) => n,
                other => panic!("sample {i} of `{expr}` is {other:?}"),
            })
            .collect())
    })
    .unwrap()
}

fn same_bits(expr: &str, expected: fn(f64, f64) -> f64) {
    let got = lua_results(expr);
    for (i, got) in (1..=SAMPLES).zip(got) {
        let (x, y) = inputs(i);
        let want = expected(x, y);
        assert!(
            got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
            "`{expr}` at x={x} y={y}: lua {got:e} ({:#x}), libm {want:e} ({:#x})",
            got.to_bits(),
            want.to_bits()
        );
    }
}

#[test]
fn the_power_operator_is_libm_pow() {
    same_bits("(x * x + 0.5) ^ y", |x, y| libm::pow(x * x + 0.5, y));
    same_bits("x ^ 3", |x, _| libm::pow(x, 3.0));
}

#[test]
fn a_folded_constant_power_is_libm_pow() {
    // Both operands are literals, so the compiler folds this one.
    same_bits("2.5 ^ 0.3", |_, _| libm::pow(2.5, 0.3));
}

#[test]
fn exp_and_log_are_libm() {
    same_bits("math.exp(y)", |_, y| libm::exp(y));
    same_bits("math.log(x * x + 0.5)", |x, _| libm::log(x * x + 0.5));
    same_bits("math.log(x * x + 0.5, 2)", |x, _| libm::log2(x * x + 0.5));
    same_bits("math.log(x * x + 0.5, 10)", |x, _| libm::log10(x * x + 0.5));
    same_bits("math.log(x * x + 0.5, 7)", |x, _| {
        libm::log(x * x + 0.5) / libm::log(7.0)
    });
}

#[test]
fn trigonometry_is_libm() {
    same_bits("math.sin(x)", |x, _| libm::sin(x));
    same_bits("math.cos(x)", |x, _| libm::cos(x));
    same_bits("math.tan(x)", |x, _| libm::tan(x));
    same_bits("math.asin(y / 13)", |_, y| libm::asin(y / 13.0));
    same_bits("math.acos(y / 13)", |_, y| libm::acos(y / 13.0));
    same_bits("math.atan(x)", |x, _| libm::atan(x));
    same_bits("math.atan(x, y)", |x, y| libm::atan2(x, y));
}

#[test]
fn the_comparison_sees_a_single_bit() {
    // The control: one unit in the last place is a failure, not a pass.
    let got = lua_results("math.sin(x)");
    let nudged = f64::from_bits(got[0].to_bits() + 1);
    assert_ne!(nudged.to_bits(), libm::sin(inputs(1).0).to_bits());
}
