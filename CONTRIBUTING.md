# Contributing

DXMTRS is MacNeutron's fork of DXMT, with Direct3D 9 (the former mtld3d)
merged in. Changes are written by hand or with an agent; [`CLAUDE.md`](CLAUDE.md)
is the agent's entry point. This file is the operating manual: what to read
before changing anything, which gate has to be green, how to read its output,
and what makes a change land on the first review instead of the third.

It states no rule twice. Every rule has exactly one home, and this file points
at it.

## Read these first

| File | What it owns |
| --- | --- |
| [`README.md`](README.md) | What the project is, and which licence covers which part of the tree. |
| [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) | Building DXMT and the Rust crates, the instruction-set baselines, the variables the build scripts read, and `tools/rust-check.sh`. |
| [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md) | Every code rule: module layout, visibility, data-structure discipline, unsafe discipline, doc-comment shape, dependencies. |
| [`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md) | Direct3D 9: how to read a test run, the conformance suite, benchmarks, and the companion edits that belong in the same change. |
| [`mtld3d.conf`](mtld3d.conf), [`dxmt.conf`](dxmt.conf) | Every runtime option, its default, and a short why. |

Read [`tests/d3d9/conformance/CONFORMANCE.md`](tests/d3d9/conformance/CONFORMANCE.md)
before changing anything in the D3D9 render, state or shader-emission path. It is
where the reasoning behind the current behaviour lives, and a change that looks
like a fix is often a divergence that was measured and kept.

## The gate

`tools/rust-check.sh` is the gate for the Rust in the tree, and it has to be
green before you commit. It runs the nightly `cargo fmt --check`, clippy with
`nursery` and `pedantic` on `aarch64-apple-darwin` and on every
`-pc-windows-msvc` target a crate ships on, `tools/audit.sh`, `cargo doc`, and
the host unit tests (the `MTLD3D_PERF=1` legs included, since no other leg
compiles the perf telemetry or the tests behind `cfg(perf_tracking)`). It prints
`rust-check: ok` when every leg passed. The variables it reads, the toolchains
and the Wine trees, are listed in
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md#rust). Only the check legs deny
warnings, so a plain `cargo clippy` in an editor reports without failing. Each
audit finding names the section of `docs/CONVENTIONS.md` behind it; read that
section rather than pattern-matching your way past the grep.

`rustfmt.toml` uses unstable options, so formatting needs the pinned nightly
(`MTLD3D_RUST_NIGHTLY` in MacNeutron's `dxmt/pins`). If a toolchain bump
reformats files you never touched, that churn is its own commit, not a
hand-revert and not a passenger in yours.

`tools/audit.sh --file <path>` runs the per-file subset of the audit, so an
agent runner or an editor can call it after every edit and surface a violation
while you write rather than at commit time. Such a hook, and any digest of the
conventions an agent runner prints at session start, is optional local setup:
the tree tracks neither, nothing keeps a digest in step with
`docs/CONVENTIONS.md`, and `tools/rust-check.sh` is the gate either way.

The functional gates (the launcher's checks, the VM gates, D3D9's conformance
run) belong to MacNeutron, which builds and runs this tree. D3D9's are described in
[`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md).

## Standing rules worth knowing before you write code

The full set is in `docs/CONVENTIONS.md`. These are the ones a newcomer trips:

- No new lint suppression. `docs/CONVENTIONS.md` lists the accepted ones, and
  complexity lints are never suppressed: introduce a parameter struct instead.
- No silent failures. Every stub, fallback and catch-all arm logs once.
- No `pub(crate)`, no `mod.rs`, no type aliases, no glob imports, no raw
  Objective-C selectors.
- Hash maps use `FxHashMap`; content hashing uses xxh3.
- Comments state the invariant, not the history that produced it: no incident
  provenance, no citations of another implementation's tests, no absolute paths
  from someone's machine. The audit greps for these.
- No em dashes in prose, comments, commit messages or documentation.
- A new `Clone` or `Copy` derive updates `tools/derive_inventory.txt`
  (`tools/audit.sh --update-derives`).

D3D9's own layering rules, and the companion edits its changes carry, are in
[`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md).

## Changes

A commit subject is `<area>: sentence`. The area is the component the change is
in (`d3d9`, `winemetal`, `d3d11`, `nvapi`, ...), or `docs`, `build`, `tools`, or
`all` for a change that spans the tree. The sentence describes the change and does
not start with a capital letter.

One change is one clearly defined unit, and it has to stay bisectable. No
drive-bys: an unrelated cleanup, a rename or a reformat picked up along the way
goes in a commit of its own. It keeps the review small and keeps the history at
one change per commit.

For anything non-trivial the commit message, or the pull request description when
there is one, carries:

1. The user-visible symptom, quoting the exact log line or error where there is
   one.
2. The root cause, the mechanism, and say so when it is conjecture.
3. The fix, in its minimum conceptual steps.
4. At least one considered alternative, including the dead ends.
5. Verification: the commands you ran and what a reviewer should look for.

### What sends a change back

- `tools/rust-check.sh` is red.
- A new lint suppression, or a new runtime environment variable.
- Incident provenance or a private path in a comment.
- Two unrelated changes in one commit.
- A behaviour change with no stated reason, or one that diverges from a Direct3D
  specification without being written down as a decision.
- For D3D9: the items [`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md) lists under
  "What sends a D3D9 change back".

## Contributing to upstream DXMT

This fork is never proposed upstream. DXMT's own rules for contributions to
3Shain/dxmt are kept here as they are, as the rule for anything that would go
there.

### Commit Messages

We use semantic commit messages.

Format: `<type>(<scope>): <subject>`

Valid types:
- feat: A newly added feature
- fix: Just a fix
- refactor: Any changes to the implementation that do not introduce new functionality
- chore: Any changes to the code that do not change the implementation (e.g. formatting, adding comments)
- docs: For documentation only (typically .md files)
- build: For build script only
- ci: For CI script only

The scope can be any sub-directory in `src/`. If a patch involves multiple scopes, separate them by comma `, `. Scopes can be omitted if there are more than 4, but please try your best to constrain your changes into one single scope. For docs/build/ci, no need to add scope.

The subject is a setence describing the change. Do not capitalize the first letter.

And most importantly, please **sign off** your commit with your legal name.

### AI Policy

We cannot accept contributions made or co-authored by AI/LLM (Large Language Model).

You are still free to use AI to do your own research and share your findings with others (including the developers, but please don't create a PR).
