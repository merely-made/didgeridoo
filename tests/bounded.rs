use piccolo::{drive, Closure, DriveError, DriveLimits, Executor, Lua, StashedExecutor};

const HEAP: usize = 16 * 1024 * 1024;
const SMALL: u64 = 10_000;
const GROWTH: &str = "local t = {} local i = 0 while true do i = i + 1 t[i] = {} end";

fn start(lua: &mut Lua, source: &str) -> StashedExecutor {
    lua.try_enter(|ctx| {
        let closure = Closure::load(ctx, None, source.as_bytes())?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    })
    .unwrap()
}

fn limits(max_ops: Option<u64>) -> DriveLimits {
    DriveLimits {
        max_ops,
        max_heap_bytes: HEAP,
    }
}

#[test]
fn drive_reports_fuel_spent_and_distinguishes_its_errors() {
    let mut lua = Lua::core();
    let executor = start(
        &mut lua,
        "local n = 0 for i = 1, 100 do n = n + i end return n",
    );
    let spent = drive(&mut lua, &executor, limits(None)).unwrap();
    assert!(spent > 0);
    let executor = start(&mut lua, "while true do end");
    assert_eq!(
        drive(&mut lua, &executor, limits(Some(SMALL))),
        Err(DriveError::OutOfFuel { max_ops: SMALL })
    );
    let executor = start(&mut lua, GROWTH);
    assert_eq!(
        drive(&mut lua, &executor, limits(None)),
        Err(DriveError::OverHeap { max_heap_bytes: HEAP })
    );
    // A script's own error is its result, read with `take_result`, not a drive error.
    let executor = start(&mut lua, "error('boom')");
    assert!(drive(&mut lua, &executor, limits(None)).is_ok());
}

#[test]
fn refusal_texts_are_stable() {
    assert_eq!(
        DriveError::OutOfFuel { max_ops: 7 }.to_string(),
        "didgeridoo refused: the script ran past its budget of 7 operations"
    );
    assert_eq!(
        DriveError::OverHeap { max_heap_bytes: 9 }.to_string(),
        "didgeridoo refused: the script used more than 9 bytes of memory"
    );
}
