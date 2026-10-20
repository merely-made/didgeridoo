# Lua 5.4 compatibility gap plan

Date: 2026-10-19
Status: queued; investigation complete, implementation not started.

## Baseline and interpretation

The unmodified official lua-5.4.9-tests run through `cargo test --release
--test lua_suite -- --nocapture`. The recorded baseline is 32 files: 1 passes,
1 skips itself, 22 fail at execution, and 8 cannot load. These are whole-file
outcomes, not a percentage of Lua semantics implemented. A missing early call
prevents the remainder of a file from being tested. Keep this baseline intact
alongside any diagnostic runs with supplied globals or reduced snippets.

The investigation found six compile failures at unsupported `<close>` variables
(api, coroutine, cstack, files, goto, locals), and two lexer failures on extended
UTF-8 escapes (literals, utf8). Several runtime files immediately require debug
or use absent string functions. `_G` is absent; nextvar reaches roughly line 259
before its global-environment assertions fail. closure waits indefinitely for a
weak-table entry to disappear. A bounded probe retained the entry after explicit
collection and two million allocation iterations.

These observations establish gaps, not promised file-pass gains. `table` and
`math` expose broad APIs, but the suite does not establish their complete
conformance. Host facilities and PUC-Lua collector behavior need an explicit
compatibility policy; availability of weak pointers in gc-arena alone does not
establish that Lua weak tables or ephemerons are cheap to implement.

## Ordered work

| Step | Work | Completion check |
|---|---|---|
| G0 | Improve diagnostics and regression detection | Preserve raw outcomes, report the first useful error and timeout separately, and track named passing files rather than only a total floor. Add small checked-in reproductions for confirmed gaps. Keep modified diagnostic runs visibly separate from official results. |
| G1 | Basic environment and value behavior | Define `_G` as the global environment; add `rawequal`; support the string metatable and method lookup. Test environment identity, raw equality, `s:sub(...)`, and primitive `getmetatable` behavior. Rerun nextvar, strings and bwcoercion to find the next blockers. |
| G2 | Pure standard library | Implement and test string format/rep, then the pattern family (find, match, gmatch, gsub), then pack/unpack/packsize. Preserve byte-string behavior, Lua patterns, captures and error semantics. Rerun math, constructs, pm, sort and tpack; do not predict full-file passes. |
| G3 | Dynamic loading and protected execution | Design load with text input, environment and mode semantics using the existing compiler; implement xpcall and missing coroutine operations where the VM supports them. Test yields through callbacks and error handlers. State the binary-chunk policy explicitly. |
| G4 | Weak tables | Design weak keys, weak values and ephemeron reachability against gc-arena and incremental collection. Test cycles, resurrection boundaries, metatable mode changes and live entries. Replace the closure timeout with a terminating regression. |
| G5 | Lua 5.4 closing semantics | Design `<close>` across normal scope exit, errors, return, coroutine close and yielding restrictions. Add compiler/VM tests before rerunning the six blocked files. A parser-only implementation is insufficient. |
| G6 | Extended UTF-8 and remaining semantic differences | Accept the Lua escape range with byte encoding rather than Rust char conversion; test boundaries and invalid escapes, then rerun literals and utf8. Investigate stack/resource limits and GC options individually. |
| G7 | Host and implementation-specific facilities | Decide which package, require, io, os and debug surfaces the embedding API offers. Keep standalone CLI tests and ltests-dependent C API tests visibly separate. Missing host capabilities remain visible in the raw suite. |

G0 is the next bounded ticket. G1–G3 offer useful script functionality before
collector and unwinding work. G4–G5 require design review in this file before
implementation. Keep the suite sources unmodified and retain subprocess timeouts.
Never add success-returning stubs merely to advance the official pass count.

## Resuming and recording progress

Pick one step, inspect current source and the relevant suite file, add a small
regression, implement the behavior, and rerun that file plus the existing tests.
Run the complete official suite after the bounded checks. Record the new first
blockers here and update COMPATIBILITY.md only for behavior actually exercised.
Update README.md and the family registry claim only from a fresh unmodified run.

The family registry is `merelyllc.com/content/specs.toml`: spec `lua-5.4`, repo
`piccolo-fork`. Its coverage command is `cargo run --bin authority -- specs` in
the site repo. The family plan is
`merelyllc.com/docs/2026-10-18_family_spec_registry_plan.md`.
No additional coverage lanes need to exist to resume this work.
