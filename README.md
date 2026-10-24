# wlr-randr-tui

A terminal UI for configuring monitors on wlroots-based Wayland compositors (niri, sway, Hyprland, Wayfire, etc.) using `wlr-randr`.

## Screenshot
<img width="2291" height="1734" alt="image" src="https://github.com/user-attachments/assets/cfbfbc2b-57bf-4149-9037-72799aaed1f8" />


## Features

- **Monitor tabs** — one tab per connected output; switch with `←`/`→` while the tab bar is focused
- **Per-output settings** — resolution/refresh rate, scale, transform, adaptive sync, enable/disable
- **Scrollable mode picker** — `Enter` on the Mode field opens a full-screen list of all supported resolutions and refresh rates, with preferred/current markers
- **Unified layout section** — position configuration is separate from per-monitor settings; one view shows all outputs at once
- **Smart relative positioning** — place a monitor relative to another with optional centering:
  - `Below DP-1`, `Below DP-1 (center)`, `Right of DP-1 (center)`, etc.
  - Centered alignment computes the exact pixel offset from logical sizes (`resolution ÷ scale`), so a 2880×1800 laptop at scale 1.75 is correctly centered under a 3440×1440 external monitor
  - The resolved absolute coordinates are shown live before you apply
- **Live layout diagram** — ASCII box diagram of all outputs at their computed positions, updates in real time
- **Projection presets** — one keypress to switch between common configurations:
  - `Extend` — all outputs on, external at origin, laptop centered below
  - `Ext Only` — laptop screen off
  - `Laptop Only` — external screen off
  - `Custom` — individual settings
- Pending changes marked with `[*]`; reset per-output with `r`
- Changes applied atomically via a single `wlr-randr` invocation; full layout always sent to avoid compositor reflow
- **Persistent profiles** — settings are saved automatically on apply and re-applied on startup, keyed by the monitor's hardware identity (EDID make/model/serial, physical size as fallback) instead of the connector name, so they survive reboots and `DP-1` ↔ `DP-2` renumbering

## Requirements

- `wlr-randr` in `$PATH`
- A wlroots-based Wayland compositor

> **Note on mirroring:** The `wlr-output-management` protocol has no concept of framebuffer cloning. Duplicate/mirror mode is not achievable through `wlr-randr` and is not implemented.

## Installation

### NixOS flake

Add to your flake inputs:

```nix
inputs.wlr-randr-tui.url = "github:youruser/wlr-randr-tui";
```

Then include the package:

```nix
environment.systemPackages = [
  inputs.wlr-randr-tui.packages.${pkgs.system}.default
];
```

### Pre-built static binary

Download the latest binary from the [Releases](../../releases) page. The binary is fully statically linked (musl) and has no runtime dependencies.

```bash
chmod +x wlr-randr-tui
./wlr-randr-tui
```

### Build from source

**NixOS / nix:**
```bash
nix develop
cargo build --release
```

**Arch:**
```bash
sudo pacman -S rust ncurses
cargo build --release
```

**Debian / Ubuntu:**
```bash
sudo apt install cargo libncurses-dev
cargo build --release
```

Binary will be at `target/release/wlr-randr-tui`.

## Usage

```
wlr-randr-tui
```

Saved profiles are applied automatically at startup. Pass `--no-restore` (or `-n`) to start from the compositor's current state instead.

### Navigation

The UI has three focus areas cycled with `Tab`:

| Focus | What it controls |
|-------|-----------------|
| **Tabs** (top bar) | Switch between monitors with `←`/`→` |
| **Settings** (middle) | Per-monitor fields: mode, scale, transform, adaptive sync, enabled |
| **Layout** (bottom) | Position of each monitor relative to others; projection presets |

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: Settings → Layout → Tabs |
| `↑` / `↓` | Navigate fields (Settings) or select monitor (Layout) |
| `←` / `→` | Switch monitor tab (Tabs), cycle values (Settings), cycle position (Layout) |
| `Enter` | Open mode picker (Mode field), enter text input (Scale), toggle boolean fields, type raw `x,y` (Layout) |
| `Esc` | Cancel edit / close mode picker |
| `p` or `.` | Cycle projection preset forward |
| `,` | Cycle projection preset backward |
| `a` | Apply all pending changes and save the resulting profiles |
| `r` | Reset pending changes for the selected monitor |
| `d` | Forget the saved profile for the selected monitor |
| `q` | Quit |

### Layout section — positions

- `↑`/`↓` selects which monitor's position to edit (it becomes highlighted in the diagram)
- `←`/`→` cycles through options: `Below X`, `Below X (center)`, `Right of X`, `Right of X (center)`, `Above X`, `Above X (center)`, `Left of X`, `Left of X (center)`, and the current absolute coordinates
- `Enter` opens a text prompt for a raw `x,y` value (e.g. `3440,0`)
- The resolved absolute position is always shown inline, e.g. `Below DP-1 (center)  -> (897, 1440)`

### Mode picker

Press `Enter` on the **Mode** field to open a scrollable list of all supported resolutions and refresh rates. Navigate with `↑`/`↓`, confirm with `Enter`, cancel with `Esc`.

## How it works

Settings are staged locally and only sent when you press `a`. On apply, the complete layout (all outputs with their resolved positions) is sent to `wlr-randr` in a single atomic call — partial updates are never used, which prevents the compositor from reflowing the layout unexpectedly.

Relative positions are resolved to absolute coordinates at apply time using each monitor's logical size (`mode pixels ÷ scale factor`), so centering accounts for HiDPI scaling correctly.

## Persistence

Every successful apply that had pending changes writes the resulting settings of all connected outputs to:

```
$XDG_CONFIG_HOME/wlr-randr-tui/config    # or ~/.config/wlr-randr-tui/config
```

Set `WLR_RANDR_TUI_CONFIG` to use a different file. The format is a plain-text, hand-editable list of `[monitor]` blocks.

Each block is keyed by a stable hardware identity derived from the EDID make, model and serial number. When the serial is missing or a placeholder (`0x00000000`, `00000000`, ...), the physical size is used instead; if that is missing too, make and model alone identify the output. Only outputs with no EDID data at all (headless/virtual) fall back to the connector name. The connector name is stored for readability but is not part of the key, so unplugging, docking, or rebooting does not change which profile applies.

At startup the matching profiles are restored and applied in a single `wlr-randr` call. Stale values are skipped rather than sent blindly: a mode the monitor no longer advertises or an unknown transform leaves that field untouched while the rest is applied. Positions are stored as the resolved absolute coordinates.

Entries for monitors that are not connected at save time are kept, so profiles for different docking stations coexist. `d` removes the profile for the selected output; apply again to recreate it.

Limitation: two identical monitors with no usable serial number and the same physical size share a single profile. The `wlr-output-management` protocol does not expose anything else that would distinguish them.
