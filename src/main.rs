#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod awake;
mod autostart;
mod color;
mod engine;
mod runloop;
mod settings;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

fn main() {
    app::run();
}
