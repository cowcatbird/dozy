<p align="center"><img src="assets/dozy.png" alt="Dozy, a sleepy orange sheep" width="220"></p>

# Dozy

Keep your Mac's screen warm and awake, 24/7. Free, open source, and it lives in your menu bar.

macOS turns Night Shift off every morning and won't let you schedule it around the clock. Dozy keeps it on. It also goes further than Night Shift can, with warmer colors and dimming below the hardware minimum.

## Features

- **Night Shift that never turns off.** The moment macOS's schedule or anything else switches it off, Dozy switches it back on (measured at about 0.05 seconds).
- **Three warming methods.** Night Shift is the default. Switch in the menu:

  | Method | Warmest | Can dim | Notes |
  | --- | --- | --- | --- |
  | Night Shift | 2700 K | No | Default. Apple's own, with the most natural-looking colors. |
  | Gamma | 1200 K | Yes | Changes each display's color curve. Can look more like an orange filter. |
  | Color Filter | 1200 K | No | Uses Accessibility › Color Filters › Color Tint, which is applied at the very end of the display pipeline. |

  Gamma and Color Filter hold Night Shift off while they run so the tints don't stack, and switch it back on when you leave them.
- **Warmth presets** from 5500 K down to 1200 K, and **brightness** down to 25% for late nights.
- **Pause** for 15 minutes, an hour, or until you resume, when you need true colors for a photo or a design.
- **Global shortcuts:** `⌃⌥⇧N` toggles warming. `⌃⌥⇧↓` and `⌃⌥⇧↑` make it warmer or cooler.
- **Keep screen awake.** Keeps the computer and display awake, and stops the screen saver from kicking in. Blocking sleep alone doesn't stop the screen saver, so Dozy also nudges the pointer by zero pixels after 20 seconds of idle. Click the sheep in the menu bar to switch it on or off: it jumps while keeping the screen awake and stands still when it isn't. Right-click the sheep for everything else.
- **Launch at login.**
- **Close to zero load.** Dozy sleeps until the system reports a change, so it wakes about twice a minute and uses no measurable CPU. The tint is applied by the display hardware, so it adds no GPU work. The app is about 600 KB and uses 8 to 10 MB of memory, close to the minimum for any app with a menu bar icon.
- **Leaves no mess.** On quit, Dozy puts your gamma and color-filter settings back exactly as they were. Night Shift stays on, because that's the point.

## Install

Download the latest build from the [Releases page](https://github.com/cowcatbird/dozy/releases).

Unzip `Dozy-macos.zip` and move `Dozy.app` to Applications. Dozy isn't notarized yet, so the first time you open it, right-click the app and choose **Open**. It runs on macOS 11 or later, on Apple Silicon and Intel Macs.

## Build from source

Install Rust from <https://rustup.rs>, then:

```sh
cargo test
cargo run --release                 # run it directly
scripts/bundle-macos.sh             # build target/Dozy.app
scripts/bundle-macos.sh --universal # Apple Silicon + Intel
```

Settings are stored as JSON in `~/Library/Application Support/Dozy/settings.json`.

## How it works

Dozy works out the look the screen should have (a color temperature and a brightness, or nothing while paused) and tells the active method to converge on it. Each method checks the current state first and only writes when something drifted.

This happens whenever macOS reports something that could undo the look: a display change, waking from sleep, a Night Shift status change, or a color filter change. It also happens every 30 seconds as a safety net.

| File | Role |
| --- | --- |
| `src/app.rs` | Tray menu and clicks, shortcuts, pause |
| `src/runloop.rs` | The main-thread loop, NSApplication's own |
| `src/color.rs` | Color temperature to RGB gains |
| `src/settings.rs` | Persisted settings |
| `src/engine.rs` | The interface every method implements |
| `src/macos/` | Night Shift (CoreBrightness), gamma (CoreGraphics), Color Filter (MediaAccessibility) |
| `src/awake.rs` | Keep screen awake |
| `assets/` | Logo, app icon and menu bar icons. Regenerate with `python3 tools/make-icons.py` (needs Pillow) |

The warming methods use private Apple frameworks, the same ones System Settings uses. Dozy has been tested on macOS 26. Apple can change these frameworks in any update.

## License

MIT
