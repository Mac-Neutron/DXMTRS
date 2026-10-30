# Testing Direct3D 9

How to read a D3D9 test run, how conformance work is organised, how the
benchmarks are judged, and the companion edits that belong in the same change.
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) is the tree's operating manual and
owns the gate (`tools/rust-check.sh`); this file is its D3D9 part.

It states no rule twice. Every rule has exactly one home, and this file points
at it.

## Read these first

| File | What it owns |
| --- | --- |
| [`README.md`](README.md) | The goal, the features, the requirements, and where everything else lives. |
| [`GAMES.md`](GAMES.md) | The games tested so far and how far each one gets. |
| [`STATUS.md`](STATUS.md) | What is implemented, what is not yet, what never will be, and the divergences kept on purpose. |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | The boundary contract: thunk versus command, stable backing for pointers the unix side dereferences, typed wire values, labelling Metal objects, the threading model, the perf counters and how to read them. |
| [`CONFORMANCE.md`](../../tests/d3d9/conformance/CONFORMANCE.md) | The conformance suite: how the runner and the baseline work, what each classification means, and the current per-site audit with its rationales. |
| [`COVERAGE.md`](../../tests/d3d9/programs/COVERAGE.md) | What the end-to-end suite covers, and the stubs whose contract a test pins on purpose. |
| [`mtld3d.conf`](../../mtld3d.conf) | Every runtime option, its default, and a short why. |

## What runs where

- The host unit tests, `cargo test` over the D3D9 crates, run in
  `tools/rust-check.sh` ([`DEVELOPMENT.md`](../DEVELOPMENT.md#rust)). They are
  the only D3D9 tests the gate runs.
- The conformance suite and the checks that read the product (`d3d9-check`,
  the facade test, the VM's `d3d9-gate`) belong to MacNeutron, which builds
  and installs the tree; see "Working on conformance".
- The end-to-end programs in `tests/d3d9/programs` and their runner in
  `tests/d3d9/runner` are built as Windows programs for `i686`, `x86_64`,
  `aarch64` and run under Wine. They used to link against the Windows SDK
  libraries that xwin supplied; this tree no longer fetches xwin, so the programs
  compile (`cargo check` and clippy cover them) but do not link, and the
  runner's own unit tests run in the gate. They are built again, with llvm-mingw,
  when D3D9 moves onto the shared core. Until then the sections "Reading a test
  run" and "Benchmarks" describe what the runner does when it is given built
  programs.

Read the conformance document before changing anything in the render, state
or shader-emission path. It is where the reasoning behind the current
behaviour lives, and a change that looks like a fix is often a divergence that
was measured and kept.

## Reading a test run

The end-to-end suite is six test binaries per architecture, and the runner
in `tests/d3d9/runner` runs each one once under Wine with every test of the
binary on the number of threads `--jobs` names. It prints one
`PASS`/`FAIL`/`SKIP` line per test and a summary that counts every test, so
the summary is the thing to read; a failure is fatal to the default run
(`--no-fail-fast` reports the whole suite). One trap remains: a pipeline
reports the last stage's status, so `runner ... | tee log` returns the exit
code of `tee`. Capture with a plain redirect, `runner ... > out.log 2>&1`,
and judge the run by the runner's summary on both architectures. mtld3d's
log of each test process is a file, `<binary>-<pid>.log` under `mtld3d-logs`
next to the test executable, one per process, so one file carries the whole
suite's log.

A process that ends cleanly is still checked against libtest's own account.
Its `test result:` line counts the results it printed, and a runner tally
short of that count means a result never arrived, whatever tore it off the
pipe. The note is `libtest counted <n> results and the runner read <m>; no
outcome for: <names>`, the named tests run again in a fresh process, and the
summary's total therefore never falls short of the set the run was asked for.
A second process that reports none of them again fails them rather than
looping.

A process that ends with tests unaccounted for leaves its whole stderr in the
same directory as `<binary>-<pid>.stderr`, and every line the runner prints
about that process names the file. What it prints inline is the last fifteen
lines the process itself wrote: Wine's `fixme` lines and everything its
`dbghelp` channel prints are left out, because dozens of them surround each
backtrace and a raw tail rarely reaches back past them to the message that
says what failed. The layer's own log of that process is the other account,
and the only one of a death the layer's crash handler ended: its fatal banner,
registers and stack go there and never to stderr. The runner moves that log
to `<binary>-<pid>.layer-log` beside the stderr, because the layer keeps only
its ten newest logs and the next run would remove it, and quotes it in the
same note from the banner on when there is one. Every test process runs
under Apple's Main Thread Checker (`debug.mainThreadChecker` in the suite's
config, `ARCHITECTURE.md` says how), so a kept `.stderr` that carries a
`Main Thread Checker: UI API called on a background thread:` line names the
AppKit call the layer made off the main thread and the thread that made it;
that call is the failure, whatever the process printed after it.

Process cleanup has one two-second budget from the first termination or cleanup
attempt, including signal-error retries and the final reap. The leader stays
unreaped until the last group signal. If cleanup cannot establish exit and reap,
the runner reports the unresolved PID and exits immediately with infrastructure
code 2. A survivor may still be running: macOS adopts the remaining children and
owns their eventual reap. No background thread survives the runner to do that
work. This bounds the runner's wait/retry loops, subject to scheduling, syscalls
and diagnostic output; the kernel can also delay process exit while closing
file descriptors. No further binary runs after this failure.

An explicit driver GPU-hang report ends the leg with exit code 3 and no
verdict, even under `FAIL_FAST=0` or when the process itself exits cleanly.
The runner watches stderr while the process runs, checks the layer log before
it can launch another process, and keeps both accounts of the initiating
process. Assertions after that report are not measurements of the source: the
later results may reflect the GPU's failed state. Run the leg again once the
GPU has recovered.

When every test is accounted for but the process ends abnormally, the runner
keeps its full captured stdout, stderr and exit status together in
`<binary>-<pid>.process-log` and names it in the note. This preserves the
existing test verdicts and does not retry completed tests. The streams are
captured decoded text, not a byte-exact pipe recording. A retention error is
reported in the note without changing those verdicts. Clean successful and
expected self-exiting processes create no such file. As with stderr, the
newest ten bundles remain in the selected log directory.

Two things are worth knowing when a test process looks wrong. Once a device
has existed, `d3d9.dll` terminates the process from the
`DLL_PROCESS_DETACH` of its exit (it cannot survive the allocator's
thread-local teardown on Wine's 1 MB main-thread stack), so a test binary's
exit status is whatever that `TerminateProcess` carries: the status the
process asked to exit with, and 0 for a process that never asked. A
`FreeLibrary` after a device does not reach that detach: the first
`CreateDevice` pins the image, so it stays mapped and the process carries
on. The harness's panic hook
(`tests/d3d9/programs/src/win32.rs`) does not wait for libtest to reach its own
exit and terminates with libtest's failure code at the first failed
assertion, after the default hook has printed the report that names the
test. The tests in flight go down with the process: the runner marks the
named test failed and runs the rest again in a fresh process, and a crash or
a hang (no result for `--timeout` seconds) is charged the same way, through a
one-thread re-run of the tests that were in flight when nothing names the
culprit. So a failure costs one result and one extra process, and the
`processes` count in the summary says how many the run took: one per test
binary is a clean run.

Explicit test selections, including recovery rounds and filtered initial runs,
are split into batches that fit the Windows command-line limit. The budget
counts UTF-16 units after quoting, with flags, separators, the terminator and
an allowance for Wine's executable-path mapping. Every deferred name keeps
its intended thread width, so a serial recovery stays serial across batches.
Each batch is an actual process with its own summary and retained failure
logs; the original failure remains failed. Fail-fast and a GPU-hang report
stop before later batches. A single name that cannot fit reports a runner
error instead of repeatedly launching an impossible command.

The harness defaults to hidden borderless windows. Wine builds a framed
window's title bar and controls on the AppKit main thread, so creating and
destroying them for every rendering test serializes a parallel run.
Tests of window-style changes use `HarnessConfig::window_style` with
`WindowStyle::Framed`, as does the window-lifecycle stress test. Keep that
choice explicit when a test needs the non-client frame.

## Benchmarks

The synthetic benchmarks are `#[ignore]`d tests of the end-to-end binary, so a
normal run lists them as ignored and never runs them; the runner's `--ignored`
runs only them. The benchmark launchers set `RUST_LOG=info` and
`__CX_UNIX_RUST_LOG=info` for timed Wine processes, independent of the shell's
filters: startup identity and perf records are required inputs, even when no
warning occurs. Untimed shape processes set both variables to the pass-trace
filter described below.

The runner's `bench-ab` subcommand is the one that answers whether a change
made things slower. It runs a base build and a candidate build, each under its
own Wine and prefix (`--base-wine`, `--base-prefix`, `--base-stamp`, and the
`--cand-` set), in alternating order for `--runs` rounds (five by default, three
at least), and judges each metric pair by pair against the noise those rounds
show. A leg's round is one process running every selected benchmark in libtest's
order, which is the same in both legs, so what one benchmark leaves in the
process (the process-wide pipeline cache, the page-box pool) reaches the next
alike on both sides of each pair; the memory rows a comparison judges are each
benchmark's growth from a sample taken before its interface. That growth is
clamped at zero, and memory an earlier benchmark frees late, inside a later one,
can hide some of the later one's growth, which one process a round cannot avoid.
The two runs of one benchmark in a round are therefore a whole round process
apart rather than back to back, and with an odd `--runs` one leg goes first in
one round more than the other. Before each round's process the run measures for
half a second the CPU its busiest other processes take, and the report warns
about every round that started on a busy machine (one other process at a quarter
of a core, all of them at half, or macOS throttling for heat); run those again.
`--wait-idle 120` optionally waits up to 120 seconds before each timed process
for three consecutive quiet half-second CPU samples, using those same
thresholds. The default `0` keeps advisory sampling. A failed CPU sample cannot
count as quiet: both process reads must succeed and include the sampler itself.
Some macOS hosts do not expose `kernel_task` to `ps`; its CPU share then remains
unavailable, explicitly recorded in each idle sample, and only the
foreign-process thresholds can be checked. Each attempt stays in the round's
`idle-<binary>/sample-*.txt` files (or `idle-host/`), including rejected
samples, and `idle-<binary>.txt` records the wait and its outcome. The final
accepted sample is the ordinary `machine-<binary>.txt` that the report reads. A
timeout ends the run with exit 2 and no performance verdict, preserving
completed rounds. This checks the machine before a process starts; it cannot
promise that background work stays idle during it.

`--base-config` and `--cand-config` replace the shared `--config` for the named
leg, including an explicitly empty value; the other leg keeps the shared config.
These apply to timed tests and their untimed shape runs, with the run's own
`log.dir` appended last. For a configuration comparison of one clean build,
`--allow-same-image` permits the same recorded image identities on both legs;
use complete config strings and retain their runtime logs so the intended
difference can be checked. The host emitter does not load the renderer and does
not use these runtime settings. The runner exits 1 on a regression and 2 when the
run itself cannot be trusted, which includes the two legs running different
Wines. The runs and the report stay in the directory `--out` names, and
`bench-compare <dir>` judges one again into a report of its own, for instance
with `--accept <metric>,...` naming an exact count (a draw count, a pass count)
that the change is meant to move. The metrics a benchmark writes include the
layer's own counters, read from the `perf-kv` line of its perf windows as
`perf.*` (`bench.rs` gives the rules): the per-frame counts of work the API calls
fix, such as `perf.draws_pf` and `perf.passes_pf`, are the exact ones.

A benchmark measures one build: the i686 build under the i686 benchmark binary
is the path World of Warcraft 1.12 and 3.3.5a take, whose x86 translator runs
our 32-bit DLLs; the ARM64X build under the x86_64 benchmark binary is the arm64
Wine's x64 path, and the x86_64 build runs on an x86_64 Wine. A layout comparison
(`--<leg>-runtime` and `--<leg>-variant`, given for both legs or neither)
compares one commit's two layouts, such as its x86_64 build, emulated, against
its ARM64X build, both under the arm64 Wine; it has one commit in both legs, so
a binary the two layouts share loads as one image, which the comparison notes
instead of refusing. It runs one Wine in both legs and changes only our DLLs, so
it measures what the ARM64X build buys an x64 game. There is no pair for the
i686 build, since a 32-bit process never loads the ARM64X build.

On an arm64 Wine the benchmarks' clock is FEX's translation of `rdtsc`, a
counter read that is not ordered after earlier stores (an `mfence` does not
order it either), so stores still draining when a timed call starts are
charged to that call. A row that times one call alone between writes is
then not the call alone: `texture_streaming`'s `lockrect.p50` and
`lockrect.p99`, the new textures' `LockRect` rows, carry part of the
previous level's fill, and move with a change that only makes that write
slower. Judge those on an arm64 leg by `fill.p50` and `fill.p99`, which time
each level's `LockRect`, write and `UnlockRect` as one interval. A texture's
levels are filled one after another, so a drain that misses one level's
interval lands in the next level's, and only the last level's can leave
the row.

The per-call rows of `api_call_cost` (`ns_per_call.*`) time single calls of
8 to 85 ns, and they move with where the linker places the layer's
functions as well as with what the functions do: single setter rows have
moved by 30 to 50 % in runs whose change did not touch the call. A
comparison therefore fails a setter row only past 50 %, and a smaller
change to the cost of a call is for the frame benchmarks to show. The draw
rows (`ns_per_call.draw_*`) move far less and fail past 15 %. An A/A run
(one commit against itself) builds it twice with the same code placement, so it
does not reproduce this; it has still moved single setter rows by about 30 %,
for a reason not established (data placement is a candidate).
`RATIO_FLOOR_PER_CALL` in `tests/d3d9/runner/src/bench/compare.rs` gives the
measurements.

An exact count a change moved on purpose fails every comparison against a base
from before that change, so such a run names it in `--accept`, and only that
run: the next change to the count fails again.

The host benchmark needs no Wine: it times DXSO parsing and MSL emission on this
machine over two synthetic corpora and any shader cache `--host-corpus` names
(`host::emit_corpus`). `bench-ab` runs it too when it is given each leg's
`emit_corpus` (`--base-host`, `--cand-host`), in rounds of its own before the
others, the run's first benchmark processes (only the short Wine process that
lists the benchmarks comes before), so no benchmark's Wine process can still be
exiting on the cores it times: each leg runs its own tree's emitter, both read
the same corpus, and its MSL byte counts are exact, so a change that alters the
emitted code shows up there even when its time per shader stays inside the
noise. The same caches are what `cold_start` measures in both legs (`--corpus-dir`
stages them). A cache only one build can read (a format or schema change between
them) is skipped for both with a note. `cold_start` counts a cache's records with
the candidate's reader, so when the base's layer writes another schema the base
leg reports its timings without those counts, and their rows read as added. Any
other difference in what a benchmark ran, such as its own configuration entries
or the depth path it took, stops the comparison: only the build and the run may
differ between the legs.

After its timed rounds, each scene benchmark (one whose metrics declare its
frame in `shape` lines) runs once more per leg with the pass trace on
(`mtld3d::d3d9::passes=trace`, the rest of the layer at warn but for the
lines that name its build and the perf windows its measured frames start
on), untimed, into
`<leg>/shape/` of the run's directory; a benchmark without `shape` lines, such
as the shader-stutter one, gets a note instead. The runner stops that run
once its log holds 33 submissions after the line the benchmark prints where
its measured frames start, so it writes no metrics: its stamp and images are
read from the log's identity lines and held to the leg's stamp and to the
images of the leg's timed rounds. The report compares the most common pass shape
of the last thirty complete submissions between the legs, with every load and
store action the load/store rules decided on those passes, and a leg in which
fewer than 80 % of them agree is an untrustworthy run, exit 2. A rule that drops
a store a later pass needs makes the frame faster, not slower, so the timings
cannot catch it and this comparison does: any difference is a shape change,
which fails the run like a changed exact metric unless `--accept` names `shape`
or `shape:<bench>`.

`bench-shape --game-log <layer log> --metrics <bench-<name>.metrics>` checks a
benchmark's scene against a frame a game dumped with Ctrl+Shift+P: the pass
count, and per pass the draw count, the fixed-function share and the textures per
draw, with render-target sizes shown relative to each side's back buffer. Each
pass is also compared on its state mix, which a benchmark declares by appending
these counts to its `shape` line: `blend=`, `atest=`, `zwrite_off=`,
`cull_none=` and `cmask0=` count the draws with blending on, with alpha test on,
whose depth-write field is 0, with cull mode NONE, and whose render target 0
colour write mask is 0, and are judged as shares of the pass's draws within 10
points. `vs_sw=`, `ps_sw=`, `tex_sw=`, `blend_sw=`, `atest_sw=` and `cull_sw=`
count the draws after the pass's first whose vertex shader, pixel shader, stage-0
texture, whole blend tuple, whole alpha-test tuple or cull mode differs from the
draw before (fixed function and no texture are values like any other), and
`vs_n=`, `ps_n=` and `tex_n=` count the distinct programmable vertex and pixel
shaders and the distinct textures on any stage. Those nine are judged within
15 % of the game's count, but never closer than 5. A `shape` line without the
state keys still parses: its state mix prints as not reported and is not judged.
Run it by hand when building or reshaping a scene that stands for a game; it is
no gate.

Bench numbers come from production-profile builds with `MTLD3D_PERF=1` only:
`release` carries debug assertions that cost more than the frame does. Nothing
else may run on the machine during a benchmark: no test leg, no conformance run,
no build in another worktree, no game. Each of them takes the same cores and GPU
the numbers measure, and a verdict is only as good as the quiet it was measured
in. Run the benchmark processes with `MTL_CAPTURE_ENABLED` unset and the Metal
validation layer and HUD off: Metal's capture layer sits under every Metal call,
and [`ARCHITECTURE.md`](ARCHITECTURE.md#ctrlshiftp-three-frame-dump-and-gpu-capture)
gives its measured cost. A deliberate comparison with the capture layer runs the
benchmark binary through the runner directly; every perf window says whether it
was on (`metal_capture_layer_count`, judged exact, so legs that differ in it
fail). An A/A run of one commit against itself (or of your uncommitted changes
against the commit they sit on) shows how far the machine moves the numbers by
itself; run it when a verdict looks surprising.

Before landing a change to the render, encoder, submit or shader path, run
`bench-ab` against the previous commit on the `wow` set and put the summary line
in the change's verification. The `wow` set is the two World of Warcraft
frames, the EVENT-query throttle under the game's settings and under the D3D9
defaults, the per-call API cost, and the buffer-lock and texture-streaming
benchmarks at the game's rates (`--bench` takes whitespace-separated test-name
filters, each selecting the benchmarks whose test path contains it, so one
suspicious benchmark can be rechecked alone). A leg's round is one process that
runs all seven in libtest's order, each on a device of its own: about a second
for Wine's start, then per benchmark about 4.5 s for its device, the warm-up,
untimed frames until its second perf window opens two seconds after its first
frame, and that one window measured. The full set, the shader-stutter and
cold-start benchmarks included, takes about twice as long.

## Which suite is right when they disagree

Wine's d3d9 test suite is the spec oracle. The end-to-end suite is our own
regression harness, so its assertions can encode our past bugs rather than D3D9's
behaviour.

When a conformance-improving change breaks an end-to-end test, the first question
is whether the assertion is wrong, not whether the change should be reverted. If
the new behaviour is what D3D9 does, update or delete the assertion and keep the
fix. Revert only when the change itself is wrong, for example when it rejects an
operation that is valid.

## Speed, conformance, and divergences

Speed is the goal and conformance serves it, so a divergence that buys frame time
and breaks no game is allowed to stay. What is not allowed is a silent one.

A kept divergence is a decision with three obligations: it gets its line in
the kept-divergence list of [`STATUS.md`](STATUS.md), its rationale goes in the Kept
divergences section of [`CONFORMANCE.md`](../../tests/d3d9/conformance/CONFORMANCE.md) (plus the cluster prose where Wine's
suite has a site for it), and where a knob makes sense it is revertible from
`mtld3d.conf`. "It matches what we ship today" is not a justification for a
change, because what we ship today may itself be the divergence. Prove a claim
about observable behaviour with a test that fails before the change and passes
after, and say in the change's description which way it went.

## Reference implementations

Before inventing a render-state heuristic, a per-shader allowlist, or any
game-shaped special case, read what an established implementation does. DXVK's
`src/d3d9/` (including its shader translator under `dxso/`) is the correctness
reference and has been hardened against thousands of D3D9 titles; Wine's own
wined3d is the baseline that runs on the same machine, so a behaviour we get
wrong and it gets right is a regression by definition.

If one of them ships clean where we do not, the gap is usually a structural
feature we have not ported, not a quirk of one game. Port the structure. When
our architecture forces a deviation, say so explicitly in the change's description
instead of quietly simplifying.

## Working on conformance

Conformance runs against the product MacNeutron builds, through its script
`dxmt/mtld3d-conformance.sh`:

```sh
dxmt/mtld3d-conformance.sh                       # the arm64x leg under wine.app, diffed against the baseline
dxmt/mtld3d-conformance.sh --only visual         # runner arguments pass through
dxmt/mtld3d-conformance.sh --only device --repeat 20 --variant intel --log debug
dxmt/mtld3d-conformance.sh --update-baseline     # re-record this leg's entries
```

The script runs the x64 `d3d9_test.exe` of Wine's d3d9 tests (the one
`make wine-arm64-winetests` builds in MacNeutron) in a prefix of its own under
the Wine the product ships, through `mtld3d-conformance` in
`tests/d3d9/conformance`, and passes when only the sites listed in
MacNeutron's `dxmt/mtld3d-triaged.txt` fail anew. `eac-vm/dxmt/d3d9-gate.sh`
runs the same suite in the VM: its x64 lane is held to the macOS product's
figures at most, and its i386 lane to its baseline's failures and crashes at
most. Both lanes run one build of Wine's tests, so the `file:line` sites of the
baseline hold under either. MacNeutron's `dxmt/d3d9-check.sh` (D3D9 through the
launcher) and `dxmt/mtld3d-facade-test.sh` (the winemetal facade's tests) are the
smaller checks on the product.

The rules that are easy to get wrong:

- Classifications live only in `CONFORMANCE.md`, on its `Sites:` lines.
  `baseline.txt` is machine-owned counts and crash state; the parser rejects
  class tokens there.
- A classification records the nature of a divergence, never its difficulty or
  how much a game cares. A hard-to-fix real defect is still real.
- Re-record the baseline in the same change as the fix that moves the counts, and
  check the diff: a re-record drops flaky- or ceiling-pinned sites that happened
  to read zero in that run.
- Derive the reason for a failing site from the upstream test source and from the
  raw actual-versus-expected values, which `MTLD3D_CONFORMANCE_RAW_DIR=<dir>`
  keeps. A site name is not a description of what the test exercises.
- Run gating runs with clean shader caches, and never re-record prefix drift: the
  script pins the prefix's display state before every run for a reason.

A shader-emission or shared-crate change has wide, subtle fallout. Run the whole
suite before committing, not just the subtest you were working on.

## Companion edits that belong in the same change

Each of these rots silently when it is left for later:

- A new render-state, texture-stage-state or sampler-state consumer moves its
  slot to consumed in the matching classifier, and flips the matching caps bit.
  The classifier warnings are only useful while every warning is a real gap.
- Shader emission has a source-derived fingerprint (`src/d3d9/core/build.rs`).
  Changes under `dxso/` and to its shared MSL inputs invalidate MSL automatically;
  extend the source list when adding an emission dependency outside those paths.
  Bump the shader-cache schema for incompatible shader keys or pipeline-translation
  semantics, and the container version for binary-format changes. Programmable
  DXSO survives emitter changes; fixed-function entries rebuild on first use.
- A new config key ships with its dispatch arm, its unit test, and its entry in
  the `mtld3d.conf` sample with the default and a short why.
- A new built-in app profile ships with the rationale for every key it sets as
  the comment on its entry in `src/d3d9/core/src/app_profile.rs` and a test that
  resolves it from the version strings the shipped binary actually carries.
  [`GAMES.md`](GAMES.md) names a game's profile in its row; the settings and
  reasons stay in `app_profile.rs`.
  A profile that pins no version field is not a profile, it is a name collision
  waiting to happen.

## D3D9 rules worth knowing before you write code

The tree-wide rules are in [`CONTRIBUTING.md`](../../CONTRIBUTING.md) and
[`CONVENTIONS.md`](../CONVENTIONS.md). These are the ones D3D9 adds:

- No new `MTLD3D_*` runtime environment variable. A runtime knob is a
  `mtld3d.conf` key; a diagnostic is a narrowed `mtld3d::*` log target consumed
  through `RUST_LOG`.
- Pure logic belongs in `mtld3d-core`; `src/d3d9` is COM wiring. Runtime
  placement follows the Unix-first policy in [`ARCHITECTURE.md`](ARCHITECTURE.md);
  native orchestration reuses core logic. A COM wrapper carries a vtable
  pointer, a refcount and an opaque inner pointer, and every other field lives
  on the inner struct.
- Every integer with symbolic meaning that crosses the boundary is a typed
  value in `src/d3d9/shared`, never a bare `u32` and never a locally restated
  constant.
- Comments cite no upstream test file outside `tests/d3d9/conformance/`, where
  the citations are the data.

## What sends a D3D9 change back

In addition to what [`CONTRIBUTING.md`](../../CONTRIBUTING.md) lists:

- A conformance regression with no re-recorded baseline and no rationale.
- An end-to-end assertion deleted without saying which behaviour it contradicted.
- A behaviour change that diverges from D3D9 without being written down as a
  decision.
- A missing companion edit: classifier arm, caps bit, cache-schema bump, config
  sample entry.
