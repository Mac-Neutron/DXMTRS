# mtld3d

Direct3D 9 for Wine on macOS, backed by Metal.

mtld3d replaces Wine's `d3d9.dll`. The PE side implements the
application-facing D3D9 API and records commands for the native Unix runtime,
which translates them into Metal command buffers and submits them. The goal is
the fastest Direct3D 9 implementation for Wine on macOS. Direct3D 8 on the
same core is planned ([#788](https://github.com/athei/mtld3d/issues/788)).
Direct3D 10 and later are not this component's job: DXMT serves them, built from
the same repository ([`README.md`](../../README.md)).

Conformance serves speed: where matching D3D9 exactly would cost frame time
and no game breaks, speed wins. Those trades are listed in
[`STATUS.md`](STATUS.md#kept-divergences).

## Features

- **Shader translation.** Shader Model 1 to 3 and the fixed-function
  pipeline are translated to Metal Shading Language by mtld3d's own
  translator.
- **Multithreading.** The game's thread records each draw and state change into
  fixed records. Translation into Metal, submission and presentation run in a
  three-thread pipeline in the native Unix library. The game's `Present` waits
  until no more than the device's maximum frame latency of frames, three by
  default, are still on their way to the screen, so a GPU-bound game does not
  run further ahead and its input latency stays bounded. Those threads and the
  shader workers take no address space from a 32-bit game, and under an arm64
  Wine they run as native arm64 code.
- **Background shader builds.** Shaders and pipelines a game uses for the
  first time build on worker threads. A frame waits once for all the builds
  it needs, not once per draw, and a pipeline whose shaders are ready builds
  ahead of shaders still queued. Asynchronous compilation goes further on a
  cold cache: a draw into a target the game rebuilds every frame is skipped
  for a frame or two instead of waited for, so the game does not stutter.
  Asynchronous compilation is on by default. Turned off, those draws wait
  with the rest.
- **Shader cache.** On by default. Translated shaders and pipelines are kept
  in a file next to the game and built again before the first frame of the
  next launch, so combinations seen before do not compile during play.
  Turned off, every launch compiles on first use.
- **Frame pacing.** Vsync and the frame cap both use Metal's minimum-duration
  present. Unlike pacing the game's thread or enabling display sync on the
  Metal layer, this works with ProMotion: the panel follows whatever rate the
  game sustains below its maximum. Vsync follows the game's present interval;
  the frame cap is off by default.
- **Upscaling.** Off by default. The game can render below the presented size
  and have MetalFX's spatial scaler upscale the result. A mip LOD bias, on by
  default, keeps texture detail at the presented size; turned off, textures
  are sampled for the smaller render size.
- **HDR output.** On by default. On a display with EDR headroom the frame is
  expanded into that headroom by inverse tone mapping that follows the live
  headroom. Turned off, or on a display without EDR headroom, the frame is
  presented as SDR.
- **Software cursor.** The cursor is drawn in its own overlay window
  independently of the game's frames, so it is not tied to the frame rate
  and has hardware-cursor latency. Under HDR it is tone-mapped like the
  frame, where the macOS cursor stays at SDR brightness. Showing or hiding it
  does not delay the next present by a refresh as the hardware cursor does.
  Off by default, because its window takes the game window off the direct
  path to the display. It can be turned on whenever HDR output is active, or
  always. Turned off, the game gets the hardware cursor.
- **Game profiles.** The few games that need settings of their own get a
  built-in profile.

Every switch, with its default, is in [`mtld3d.conf`](../../mtld3d.conf).
[`STATUS.md`](STATUS.md) lists what is implemented, what is not
yet, what never will be, and the divergences from D3D9 kept on purpose.

## Requirements

- macOS 15 or 26, on Apple Silicon or Intel.
- An arm64 Wine, which runs x86 games through FEX and loads an ARM64X build of
  mtld3d into 64-bit games, is the primary target: MacNeutron's Wine is one,
  and [wine-build](https://github.com/athei/wine-build)'s is another. An
  x86_64 Wine, such as wine-build's, based on CrossOver 26, which an Intel Mac
  runs, takes the x86_64 build. Older Wine or CrossOver releases are not
  expected to work.
- macOS 26.4 or later for the arm64 Wine, whose binaries are built for it.
  On an Apple Silicon Mac below that, such as one on macOS 15, the x86_64
  Wine under Rosetta 2 is the route.
- A 64-bit prefix or bottle. 32-bit games run in it through WoW64.
- Rosetta 2 for an x86_64 Wine. An arm64 Wine translates x86 itself.
- [x87sidecar](https://github.com/athei/x87sidecar) under an x86_64 Wine.
  D3D9-era games do their floating-point math in x87 instructions, which
  Rosetta 2 translates slowly; the wine-build releases carry its patch.

## Installation

MacNeutron ships this component inside its own Wine: `d3d9.dll` and `mtld3d.dll`
as Wine builtins and `mtld3d.so` beside them, in place of the stock d3d9. There is
no standalone bundle to download any more. Wine only loads a builtin whose name
has a marker in the prefix, and `wineboot` writes the markers when it creates a
prefix, so a build installed later reaches prefixes created after it, or an
existing prefix after `wineboot -u`. The [Fullscreen](#fullscreen) section below
shows how to keep a fullscreen game's mode change virtual.

## Configuration

mtld3d reads `mtld3d.conf` from the directory of the game's `.exe` at every
`Direct3DCreate9` and keeps it for that interface and its devices. A missing
file means defaults. The `MTLD3D_CONFIG` environment variable takes the same
entries, separated by semicolons.

Each option is resolved from four layers. A later layer wins, key by key:

1. The built-in default.
2. The built-in profile for the running game, if there is one.
3. `mtld3d.conf`.
4. `MTLD3D_CONFIG`.

A profile matches on the executable name plus the version resource its
vendor linked in, and the log names the profile that matched. The
[sample `mtld3d.conf`](../../mtld3d.conf) documents every option, its default and
why it exists. [`app_profile.rs`](../../src/d3d9/core/src/app_profile.rs) lists the
profiles and the reason for each setting they make.

## Logs, games and bug reports

Every process writes `mtld3d-logs/<exe>-<pid>.log` next to the executable,
never to the standard streams. `RUST_LOG` filters it: unset, everything logs
at `info`, and `RUST_LOG=mtld3d=warn` quiets the whole project. The log
targets, the levels and the Ctrl+Shift+P frame capture are described in
[`ARCHITECTURE.md`](ARCHITECTURE.md#logging). The capture hotkey
is a letter chord because no Ctrl+Shift function-key chord is free: a bare
F12 is Steam's default screenshot key, the Metal HUD's menu takes Shift+F8 to
Shift+F12 and Ctrl+Shift+F9 to Ctrl+Shift+F12, and macOS takes its Ctrl+F1 to
Ctrl+F8 keyboard-navigation shortcuts even with Shift held, all before the
game sees the key. P sits at the same place on the common layouts (QWERTY,
QWERTZ and AZERTY). The chord takes the Control key, not Command, and holding
Command blocks it, since Wine's Mac driver reports Command as Alt.

[`GAMES.md`](GAMES.md) lists the games tested so far and
[how to report one](GAMES.md#reporting-a-game) that fails.

Every run writes two lines into its log before the game draws anything, and no
`RUST_LOG` setting is needed to see them. They say mtld3d is live:

```
[mtld3d::shim] mtld3d.dll <version> <id>, unix call initialized as <arch>
[mtld3d::d3d9] d3d9.dll <version> <id> loaded at <address> as <arch>
```

`<arch>` is the build the process runs: `i686` for a 32-bit game, `x86_64` for
the x86_64 build, `arm64ec` for the ARM64X build in an x64 game on an arm64
Wine, and `aarch64` in a native ARM64 program. If the lines are absent,
`d3d9.dll` never mapped. Check the prefix's markers first: `mtld3d.dll` has to
exist in the prefix's `syswow64` (32-bit game) or `system32` (64-bit game),
because `d3d9.dll` imports it.

## Building from source

D3D9 is built by the same meson build as the rest of DXMTRS, in a cross build:
meson runs cargo for each crate and target, links the DLLs, and installs them
with the other builtins. [`DEVELOPMENT.md`](../DEVELOPMENT.md#rust) covers the
prerequisites, which Rust target builds which binary, and `tools/rust-check.sh`;
[`TESTING.md`](TESTING.md) covers the tests.

## Fullscreen

A fullscreen game sets the display mode it picked, as on Windows, and gets a
borderless window covering the monitor. To keep that mode-set virtual, so the
desktop keeps its resolution and other windows stay where they are, set Wine's
`EmulateModeset` key in the prefix:

```sh
wine reg add 'HKCU\Software\Wine\X11 Driver' /v EmulateModeset /d Y /f
```

win32u reads the key whatever the driver, once per Wine session, so set it
before launching the game and let the previous session exit first. With it the
game window is scaled onto the physical display, letterboxed if the aspect
differs, and mouse input is mapped into the mode. Without it, Wine's mac
driver switches the whole desktop to the game's mode.

On a prefix whose system DPI is set to a value other than 96, Wine scales the
window size and the mouse coordinates of a game that does not declare itself
DPI-aware by 96 over that DPI, but not the display mode list. In fullscreen
the game then lays out its interface in a fraction of the screen, its clicks
land away from the drawn pointer, and the pointer cannot reach the far edges.
mtld3d logs a warning that names the mismatch when such a game goes
fullscreen. The remedy is to mark the executable DPI-aware in the prefix,
under its file name without the path:

```sh
wine reg add 'HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers' /v hl2.exe /d '~ HIGHDPIAWARE' /f
```

With `/ve` in place of `/v hl2.exe` the value becomes the key's default and
applies to every program in the prefix; that default is a Wine extension,
which Windows does not read. The flag is read when a process starts, so
restart the game after setting it.

The resolution picked in the game's video options therefore sizes the frame.
`render.scale` in `mtld3d.conf` multiplies on top of it, rendering fewer
pixels and upscaling the result to the screen.

The resolution list a game sees describes the primary display. The list the
adapter reports carries the sizes that fill the display, largest first (those
that, scaled onto it, leave a bar of less than one pixel), and then the
standard sizes of another shape that Wine lists for it (2560x1440, 1920x1080,
1600x900, 1280x720, 1024x768, 800x600 and 640x480), largest first, which are
letterboxed in fullscreen. It holds at most 15 sizes per colour format,
because Wine's full list overflows menus built for a driver's short one. A
game that builds its menu from Wine's own display-mode list is shown the same
sizes in Wine's order, and a game that sorts its menu shows its own order. Any
mode Wine accepts stays settable whether listed or not, except the sizes some
Wine builds abort on ([`STATUS.md`](STATUS.md#kept-divergences)). A request that
matches no mode, such as a size a game derived from its own window, follows
the window instead. A fullscreen game is never told it lost its device on a
focus change: the desktop mode comes back on deactivation and the game's mode
is set again on activation.

## Architecture

![Component diagram: game.exe calls d3d9.dll through the D3D9 COM API;
d3d9.dll, which links mtld3d-core, records each frame and calls the function
mtld3d_unix_call exported by mtld3d.dll; mtld3d.dll crosses the Wine PE/Unix
boundary into mtld3d.so, the native runtime, whose encoder, submit, presenter
and worker threads drive Metal. The PE side is i386 or x86_64, one chain per
architecture, and on an arm64 Wine an x64 game loads ARM64X builds of
d3d9.dll and mtld3d.dll instead; the host side is Mach-O in Wine's own
architecture.](architecture.svg)

`d3d9.dll` implements the COM API, keeps the state the game can observe, and
records each frame into fixed records. `mtld3d.dll` is the PE shim that owns
Wine's unix-call globals. `mtld3d.so` is the native runtime on the host: an
encoder thread translates the recorded frame into Metal commands, a submit
thread commits them, a presenter thread presents, and worker threads compile
shaders and pipelines and prewarm the shader cache.
[`ARCHITECTURE.md`](ARCHITECTURE.md) has the boundary contract, the
threading model and the debugging toolkits.

## Contributing

[`CONTRIBUTING.md`](../../CONTRIBUTING.md) is the operating manual,
[`TESTING.md`](TESTING.md) is D3D9's testing manual, and
[`CONVENTIONS.md`](../CONVENTIONS.md) holds the code rules.

## License

[zlib](LICENSE). The files of this component are altered from
[athei/mtld3d](https://github.com/athei/mtld3d) 0.14.1, and the repository's
[`README.md`](../../README.md#licences) says which licence covers which file.
