# Lua 5.4 compatibility findings and dependencies

Date: 2026-10-19
Status: investigation notes; no implementation order or work commitment.

## Baseline and interpretation

The unmodified official lua-5.4.9-tests run through `cargo test --release
--test lua_suite -- --nocapture`. The recorded baseline is 32 files: 1 passes,
1 skips itself, 22 fail at execution, and 8 cannot load. These are whole-file
outcomes, not a percentage of Lua semantics implemented. A missing early call
prevents the remainder of a file from being tested. Diagnostic runs with supplied
globals or reduced snippets provide different evidence from the unmodified run.

The investigation found six compile failures at unsupported `<close>` variables
(api, coroutine, cstack, files, goto, locals), and two lexer failures on extended
UTF-8 escapes (literals, utf8). Several runtime files immediately require debug
or use absent string functions. `_G` is absent; nextvar reaches roughly line 259
before its global-environment assertions fail. closure waits for a weak-table
entry to disappear and times out. A bounded probe retained the entry after
explicit collection and two million allocation iterations.

These observations establish gaps, not promised file-pass gains. `table` and
`math` expose broad APIs, but the suite does not establish their complete
conformance. Host facilities and PUC-Lua collector behavior involve compatibility
policy choices. Availability of weak pointers in gc-arena alone does not
establish that Lua weak tables or ephemerons are cheap to implement.

## Gaps and the behavior they touch

The rows are a map of related findings, not a priority order.

| Area | Finding | Dependencies and useful evidence |
|---|---|---|
| Diagnostics | The runner reports the first failure per file and guards a total pass floor. | Named passing files would expose regressions hidden by a compensating gain elsewhere. First-error details and separate timeout reporting would make whole-file counts easier to interpret. Small reproductions isolate behaviors hidden by earlier failures. |
| Environment and value behavior | `_G`, `rawequal` and string metatable behavior are missing. | Global environment identity affects nextvar. String method lookup and primitive getmetatable behavior affect strings and bwcoercion. Environment identity, raw equality and `s:sub(...)` are small independent probes. |
| Pure standard library | String format/rep, patterns and packing functions are missing. | math and constructs reach formatting; pm and sort reach patterns; tpack reaches packing. Lua byte strings, patterns, captures and error behavior constrain compatibility. Filling an early API exposes later assertions; it does not predict a file pass. |
| Dynamic loading and protected execution | load, xpcall and some coroutine operations are absent. | The compiler provides a possible basis for text loading. Environment/mode arguments and binary chunks involve API choices. Yields through callbacks and error handlers interact with execution semantics. |
| Weak tables | The weak entry in the bounded probe survives collection; closure waits for that behavior. | Weak keys, weak values and ephemeron reachability interact with incremental collection. Cycles, resurrection, metatable mode changes and live entries distinguish correct reachability from simply clearing entries. |
| Closing semantics | `<close>` prevents six suite files from compiling. | Scope exit, errors, return, coroutine close and yielding restrictions all affect the implementation. Syntax acceptance alone would not establish closing behavior. |
| Extended UTF-8 and other semantics | Two files contain escapes above U+10FFFF that the lexer rejects. Stack/resource limits and GC options also differ or remain unresolved. | Lua's escape range requires byte encoding beyond Rust char values. Boundary/invalid-escape cases isolate the lexer issue. Resource and GC differences need separate probes. |
| Host facilities | package, require, io, os and debug coverage is absent or limited. | The intended embedding surface determines which facilities make sense. Standalone CLI tests and ltests-dependent C API tests exercise additional capabilities. Their absence remains visible in the raw result. |

## Recovering the evidence

The suite lives under `tests/lua-5.4-tests`, pinned by its VERSION file. The
runner is `tests/lua_suite.rs`; it uses subprocess timeouts. COMPATIBILITY.md
records API-level claims, and README.md records the whole-file baseline.
Modified diagnostic scripts and reduced snippets from the investigation were
exploratory; this document records their findings rather than promising a
reproducible checked-in diagnostic suite.

The family registry is `merelyllc.com/content/specs.toml`: spec `lua-5.4`, repo
`piccolo-fork`. Its coverage command is `cargo run --bin authority -- specs` in
the site repo. The family notes are
`merelyllc.com/docs/2026-10-18_family_spec_registry_plan.md`.

These notes preserve what was found and where to look again. Current source,
new evidence and the needs of the moment can change which gaps matter and how
they relate. The filename is retained for existing links; it assigns no next task.
