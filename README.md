# wlr-randr-tui

A terminal UI for configuring monitors on wlroots-based Wayland compositors (niri, sway, Hyprland, Wayfire, etc.) using `wlr-randr`.

## Screenshot

```
 wlr-randr TUI
 Proj: [Extend] [External Only] [Laptop Only] [Custom]  p / , . : cycle
-------------------------------------------------------------------------------
 Monitors              | DP-1
 > [M] DP-1            |   Make:  Dell Inc.
   [L] eDP-1 *         |   Model: DELL P3425WE
                       | ------------------------------------------------------
                       | > Mode:           3440x1440 @ 59.973 Hz (current)
                       |   Position:       x=0 y=0
                       |   Scale:          1.0000
                       |   Transform:      normal
                       |   Adaptive Sync:  disabled
                       |   Enabled:        yes
                       | ------------------------------------------------------
                       |  Layout:
                       |  +-----DP-1------+
                       |  |              |
                       |  +------+-------+
                       |       +--eDP-1--+
                       |       |         |
                       |       +---------+
-------------------------------------------------------------------------------
 Tab:panel  Up/Dn:nav  Left/Right:cycle  Enter:edit  p:proj  a:apply  r:reset  q:quit
```

## Features

- Browse all connected outputs and their full mode lists
- Per-output settings: resolution/refresh, position, scale, transform, adaptive sync, enable/disable
- **Smart relative positioning** — place a monitor relative to another with alignment:
  - `Below DP-1`, `Below DP-1 (center)`, `Right of DP-1 (center)`, etc.
  - Centered alignment computes the exact pixel offset from logical sizes (`resolution ÷ scale`), so a 2880×1800 laptop at scale 1.75 is correctly centered under a 3440×1440 external monitor
  - The resolved absolute coordinates are shown live before you apply
- **Layout preview** — ASCII diagram of all outputs at their computed positions, updates in real time as you change settings
- **Projection presets** — one keypress to switch between common configurations:
  - `Extend` — all outputs on, external at origin, laptop centered below
  - `External Only` — laptop screen off
  - `Laptop Only` — external screen off
  - `Custom` — individual settings
- Scrollable mode picker (Enter on Mode field) showing all available resolutions and refresh rates with preferred/current markers
- All pending changes marked with `[*]`; reset per-output with `r`
- Changes applied atomically via a single `wlr-randr` invocation

## Requirements

- `wlr-randr` in `$PATH`
- A wlroots-based Wayland compositor
- `ncurses` (runtime)

> **Note on output duplication ("Mirror"):** The `wlr-output-management` protocol has no concept of framebuffer cloning. Duplicate/mirror mode is not achievable through `wlr-randr` and is not implemented.

## Building

### NixOS

```bash
nix-shell --run "cargo build --release"
```

A `shell.nix` is included with all required dependencies.

### Other distros

```bash
# Arch
sudo pacman -S rust ncurses

# Debian/Ubuntu
sudo apt install cargo libncurses-dev

cargo build --release
```

Binary will be at `target/release/wlr-randr-tui`.

## Usage

```
wlr-randr-tui
```

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` | Switch between monitor list and settings panel |
| `↑` / `↓` | Navigate monitors or settings fields |
| `←` / `→` | Cycle through values (mode, position, scale, transform, etc.) |
| `Enter` | Open mode picker (on Mode field), enter text input (Position, Scale), or toggle (Adaptive Sync, Enabled) |
| `Esc` | Cancel edit / close mode picker |
| `p` or `.` | Cycle projection preset forward |
| `,` | Cycle projection preset backward |
| `a` | Apply all pending changes |
| `r` | Reset pending changes for the selected monitor |
| `q` | Quit |

### Position field

- `←` / `→` cycles through relative options: `Below X`, `Below X (center)`, `Right of X`, `Right of X (center)`, `Above X`, `Above X (center)`, `Left of X`, `Left of X (center)`, and your current absolute coordinates
- `Enter` opens a text prompt for a raw `x,y` value (e.g. `3440,0`)
- The computed absolute pixel position is shown inline, e.g. `Below DP-1 (center)  -> (897, 1440)`

### Mode picker

Press `Enter` on the **Mode** field to open a scrollable list of all supported resolutions and refresh rates. Preferred and currently active modes are marked. Navigate with `↑`/`↓`, confirm with `Enter`, cancel with `Esc`.

## How it works

Settings are staged locally and only sent when you press `a`. The final `wlr-randr` command is shown in the status bar on success. Relative positions are resolved to absolute coordinates at apply time using each monitor's logical size (`mode pixels ÷ scale factor`), so centering accounts for HiDPI scaling correctly.
