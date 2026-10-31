> **MacNeutron's fork of DXMT.** This fork is maintained for [MacNeutron](https://github.com/chadouming/MacNeutron)
> and is not affiliated with the DXMT project. Changes on the `macneutron` branch are AI-assisted and, per DXMT's
> contribution policy, are never proposed upstream. Please report problems with this fork to MacNeutron, not to DXMT.

# DXMT

A Metal-based translation layer for Direct3D 11 and 10 which allows running 3D applications on macOS using Wine.

For the current status of the project, please refer to the [project wiki](https://github.com/3Shain/dxmt/wiki).

The most recent development builds can be found [here](https://github.com/3Shain/dxmt/actions).

## Direct3D 9

The tree also holds a Direct3D 9 implementation for Wine on macOS, backed by Metal. It is the former
[mtld3d](https://github.com/athei/mtld3d), version 0.14.1, altered here: it lives in `src/d3d9` with its tests in
`tests/d3d9`, builds with this tree's one Cargo workspace and one meson build, and still has its own runtime beside
DXMT's. [`docs/d3d9/README.md`](docs/d3d9/README.md) describes it; [`mtld3d.conf`](mtld3d.conf) lists its options.


## Build

See [DEVELOPMENT.md](docs/DEVELOPMENT.md)

## Licences

DXMTRS is a work derived from DXMT and is licensed under the GNU Lesser General Public License, version 2.1 or later
([`LICENSE`](LICENSE), [`COPYING.LIB`](COPYING.LIB)); DXMT up to v0.80 was MIT ([`LICENSE.OLD`](LICENSE.OLD)). Parts that
came from elsewhere keep their own notices, and this is where they are:

- **Direct3D 9 is zlib.** Its files are altered from athei/mtld3d 0.14.1 and are plainly marked as altered here, as the
  licence asks. The notice travels with them: an unaltered `LICENSE` in each of [`src/d3d9/`](src/d3d9/LICENSE),
  [`tests/d3d9/`](tests/d3d9/LICENSE) and [`docs/d3d9/`](docs/d3d9/LICENSE), and `license = "Zlib"` in the manifest of
  every D3D9 crate. A copy beside each of the following would be noise, so they are named here instead: `CLAUDE.md` (and
  `AGENTS.md`, which links to it), `CONTRIBUTING.md` (but for its "Contributing to upstream DXMT" section, which is
  DXMT's), `docs/CONVENTIONS.md`, `mtld3d.conf`, `rustfmt.toml`, `tools/audit.sh`, `tools/derive_inventory.txt` and
  `tools/arm64x-link.sh` (ported from mtld3d's Makefile). `tools/mingw-cc.sh` and `tools/linux-cargo.sh` are
  MacNeutron's own and carry the tree's licence.
- **`vendor/snmalloc-sys` is MIT.** It is the snmalloc-sys crate with Microsoft's snmalloc (also MIT) in `upstream/`,
  patched for ARM64EC; the crate's notice is [`vendor/snmalloc-sys/LICENSE`](vendor/snmalloc-sys/LICENSE) and
  snmalloc's is [`vendor/snmalloc-sys/upstream/LICENSE`](vendor/snmalloc-sys/upstream/LICENSE). D3D9's allocator comes
  from it.
- **`src/util/rust_pe_rt.c` is LGPL, like the rest of the tree, and is linked into D3D9's guest DLLs.** The
  Rust runtime shim for `-pc-windows-msvc` code is part of the x86_64 and i386 `d3d9.dll` and `mtld3d.dll` the VM
  guest loads, which are otherwise zlib. The source offer for that object is this repository.
- Everything else is under the LGPL above unless the file or its folder says otherwise.
