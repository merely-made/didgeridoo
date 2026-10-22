//! Runs the official Lua 5.4 test suite (vendored in `tests/lua-5.4-tests`) one file at a time and
//! reports pass / fail / cannot-load per file.
//!
//! Most files are expected to fail on unimplemented stdlib functions, so the test only checks that
//! the pass count has not fallen below `PASS_FLOOR`. Raise it when more files pass.
//!
//! Each file runs in a child process (this test binary re-invoked with `LUA_SUITE_FILE` set) so
//! that panics and hangs are contained and counted as failures.

use std::{
    fs::{read_dir, File},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use piccolo::{io, Closure, Executor, Lua};

const SUITE_DIR: &str = "tests/lua-5.4-tests";
/// The counts the family spec registry reads; set `LUA_SUITE_RECORD=1` to rewrite them.
const RESULTS_FILE: &str = "spec-results.toml";
const TIMEOUT: Duration = Duration::from_secs(20);
const EXIT_FAIL: i32 = 1;
const EXIT_CANNOT_LOAD: i32 = 2;

/// Current number of suite files that pass without skipping themselves.
const PASS_FLOOR: usize = 1;

#[derive(Debug, PartialEq)]
enum Outcome {
    Pass,
    /// Exited cleanly, but only because the file skips itself without the `ltests` C library.
    Skipped,
    Fail(String),
    CannotLoad(String),
}

/// Child mode: run one suite file, exiting with a code that encodes the outcome.
fn run_one(file: &str) -> ! {
    let mut source = Vec::new();
    io::buffered_read(File::open(file).unwrap())
        .unwrap()
        .read_to_end(&mut source)
        .unwrap();

    let mut lua = Lua::full();
    let loaded = lua.try_enter(|ctx| {
        let closure = Closure::load(ctx, Some(file), &source[..])?;
        Ok(ctx.stash(Executor::start(ctx, closure.into(), ())))
    });
    let exec = match loaded {
        Ok(exec) => exec,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(EXIT_CANNOT_LOAD);
        }
    };
    match lua.execute::<()>(&exec) {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(EXIT_FAIL);
        }
    }
}

fn last_line(stderr: &str) -> String {
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    line.trim().chars().take(110).collect()
}

fn run_in_child(file: &str) -> Outcome {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lua_suite_scripts", "--nocapture"])
        .env("LUA_SUITE_FILE", file)
        .current_dir(SUITE_DIR)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stderr = child.stderr.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let out_reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        String::from_utf8_lossy(&text).into_owned()
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let reason = last_line(&reader.join().unwrap());
    let skipped = out_reader.join().unwrap().contains("testC not active");

    match status {
        None => Outcome::Fail(format!("timeout after {}s", TIMEOUT.as_secs())),
        Some(s) if s.success() && skipped => Outcome::Skipped,
        Some(s) if s.success() => Outcome::Pass,
        Some(s) if s.code() == Some(EXIT_CANNOT_LOAD) => Outcome::CannotLoad(reason),
        Some(s) if s.code() == Some(EXIT_FAIL) => Outcome::Fail(reason),
        Some(s) => Outcome::Fail(format!("abnormal exit ({s}): {reason}")),
    }
}

#[test]
fn lua_suite_scripts() {
    if let Ok(file) = std::env::var("LUA_SUITE_FILE") {
        run_one(&file);
    }

    // `all.lua` is the suite's own driver (dofile/os.execute); the other files are run directly.
    let mut files: Vec<String> = read_dir(Path::new(SUITE_DIR))
        .expect("could not list suite dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".lua") && name != "all.lua")
        .collect();
    files.sort();

    let mut out = std::io::stdout();
    let (mut pass, mut skipped, mut fail, mut cannot_load) = (0, 0, 0, 0);
    for file in &files {
        let outcome = run_in_child(file);
        let (tag, reason) = match &outcome {
            Outcome::Pass => {
                pass += 1;
                ("pass", "")
            }
            Outcome::Skipped => {
                skipped += 1;
                ("skipped", "needs ltests")
            }
            Outcome::Fail(r) => {
                fail += 1;
                ("fail", r.as_str())
            }
            Outcome::CannotLoad(r) => {
                cannot_load += 1;
                ("cannot-load", r.as_str())
            }
        };
        let _ = writeln!(out, "{tag:<11} {file:<16} {reason}");
    }
    let _ = writeln!(
        out,
        "lua-5.4 suite: {} files, {pass} pass, {skipped} skipped, {fail} fail, {cannot_load} cannot-load",
        files.len()
    );

    check_recorded(pass, files.len());
    assert!(
        pass >= PASS_FLOOR,
        "lua suite regressed: {pass} files pass, floor is {PASS_FLOOR}"
    );
}

/// Keep `spec-results.toml` truthful: every test run either matches it or, with
/// `LUA_SUITE_RECORD=1`, rewrites it with today's date.
fn check_recorded(pass: usize, files: usize) {
    let counts = format!("files_all_pass = {pass}, files = {files} }}");
    let text = std::fs::read_to_string(RESULTS_FILE).expect("spec-results.toml is missing");
    if std::env::var_os("LUA_SUITE_RECORD").is_some() {
        let line = text
            .lines()
            .find(|line| line.starts_with("measured = "))
            .expect("spec-results.toml has no measured line");
        let scope = &line[line.find("scope = ").unwrap()..line.find(", files_all_pass").unwrap()];
        let updated = format!("measured = {{ on = \"{}\", {scope}, {counts}", today());
        std::fs::write(RESULTS_FILE, text.replace(line, &updated)).unwrap();
    } else {
        assert!(
            text.contains(&counts),
            "spec-results.toml disagrees with this run ({pass} of {files} files pass); \
             rerun with LUA_SUITE_RECORD=1 to record it"
        );
    }
}

fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        / 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
