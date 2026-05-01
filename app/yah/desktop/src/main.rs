// Thin shim — actual Tauri builder lives in `lib.rs::run()` so it can be
// shared with the future mobile entry point.

fn main() {
    desktop::run();
}
