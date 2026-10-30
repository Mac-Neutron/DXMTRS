# Working on DXMTRS

[`CONTRIBUTING.md`](CONTRIBUTING.md) is the operating manual for this repository:
the gate, how to read its output, and what a change is expected to contain.
Read it before changing anything. D3D9 has a manual of its own for its test
runs, [`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md).

The tree is one project with one Cargo workspace at the root and one meson
build. The C++ and the Rust crates of Direct3D 10 and 11 sit under `src/`
(`src/dxgi`, `src/d3d10`, `src/d3d11`, `src/winemetal`, `src/util`, `src/nvapi`,
`src/nvngx`, ...); Direct3D 9, the former mtld3d, sits under `src/d3d9` with its
tests in `tests/d3d9`. Cargo commands run from the root.

It points at the files that own the rules:

- [`README.md`](README.md), what the project is, and which licence covers which
  part of the tree.
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md), building DXMT, the Rust crates
  and their instruction-set baselines, the variables the build scripts read,
  and `tools/rust-check.sh`.
- [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md), every code rule, with
  `tools/audit.sh` enforcing the mechanical half. State lives on an object, not in
  a static: a new `static` holding mutable state needs one of the arguments
  that section lists, written in its doc block.
- [`docs/d3d9/README.md`](docs/d3d9/README.md), Direct3D 9: the goal, the
  features, the configuration and the logs.
- [`docs/d3d9/STATUS.md`](docs/d3d9/STATUS.md), what D3D9 implements, what it
  does not, and the divergences kept on purpose.
- [`docs/d3d9/GAMES.md`](docs/d3d9/GAMES.md), the D3D9 games tested so far and
  how far each one gets.
- [`docs/d3d9/ARCHITECTURE.md`](docs/d3d9/ARCHITECTURE.md), the PE/Unix boundary
  contract of D3D9, its threading model, and its diagnostic toolkits.
- [`docs/d3d9/TESTING.md`](docs/d3d9/TESTING.md), how to read a D3D9 test run,
  how conformance work is organised, benchmarks, and the companion edits that
  belong in the same change.
- [`tests/d3d9/conformance/CONFORMANCE.md`](tests/d3d9/conformance/CONFORMANCE.md),
  the D3D9 conformance suite, the classification scheme, and the rationale
  behind every divergence we keep.
- [`tests/d3d9/programs/COVERAGE.md`](tests/d3d9/programs/COVERAGE.md), what the
  D3D9 end-to-end suite covers and which stubs it pins.
- [`mtld3d.conf`](mtld3d.conf) and [`dxmt.conf`](dxmt.conf), every runtime
  option of D3D9 and of D3D10/11 with its default.

`tools/rust-check.sh` is the Rust gate and `tools/audit.sh` is its mechanical
half. `CONTRIBUTING.md` explains how to run them and what they cover.

Every plan and every change is checked against those rules before it is
presented or committed, section by section, and the check is written down. A
plan carries a "Rules check" section that names what it introduces (a static,
a config key, a wire field, a dependency, a derive, a lint suppression, a
duplicate of something that already exists) and why each is allowed; a
change's verification names the gates that enforce those rules and ran
green. A plan or a change without that check is not ready.
