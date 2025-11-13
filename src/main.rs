//! Spill UI Framework - Main Example
//!
//! This demonstrates the recommended way to use Spill with the component system.
//! For comparison with direct view building, see examples/view_based.rs

use spill::component::context::ThemeMode;
use spill::component::examples::with_theme_holy_grail;
use spill::window::{WindowConfig, run_app};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Spill with component system...");

    let config = WindowConfig {
        title: "Spill UI Framework - Holy Grail Layout".to_string(),
        width: 1200.0,
        height: 800.0,
        clear_color: wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
    };

    // Check for DEBUG_TEXT_MODE environment variable
    let initial_theme = if let Ok(mode) = std::env::var("DEBUG_TEXT_MODE") {
        match mode.as_str() {
            "bounds" | "BoundsOnly" => {
                println!("🔧 DEBUG_TEXT_MODE=BoundsOnly - Text will render as bounds only");
                ThemeMode::Hud
            }
            _ => ThemeMode::Hud,
        }
    } else {
        ThemeMode::Hud
    };

    // Use ThemedComponent wrapper with HolyGrailV2
    // This enables theme cycling via footer tap (taps grandparent state)
    run_app(config, with_theme_holy_grail(initial_theme))
}
