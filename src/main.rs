#[cfg(not(target_os = "macos"))]
compile_error!("Dozy is macOS-only.");

mod app;
mod awake;
mod autostart;
mod color;
mod engine;
mod macos;
mod runloop;
mod settings;

fn main() {
    app::run();
}
