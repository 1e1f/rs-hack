//! # Spill - Real-time Audio UX Library
//!
//! A wgpu-based UI library designed specifically for real-time audio visualization
//! with zero-latency audio data flow and custom shader support.
//!
//! ## Core Principles
//!
//! - **Zero-latency audio visualization** using lock-free primitives
//! - **Custom WGSL shaders** per component
//! - **Smart caching** with selective invalidation
//! - **GPU-first rendering** pipeline
//! - **Multitouch-first** interaction design
//!
//! ## Architecture
//!
//! Spill provides a component-oriented API with sophisticated threading and state management:
//!
//! ### Threading Model
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌──────────────┐
//! │   Winit     │────→│  UI Thread   │────→│ Render Thread│
//! │   Thread    │     │  (Layout)    │     │   (60 FPS)   │
//! └─────────────┘     └──────────────┘     └──────────────┘
//!       │                    ↕                      │
//!       │              ┌──────────────┐            │
//!       └─────────────→│ Model Thread │            │
//!                      │ (App State)  │            │
//!                      └──────────────┘            │
//!                                                   ↓
//!                                              GPU/Screen
//! ```
//!
//! - **Winit Thread**: System event loop (OS window/input events)
//! - **UI/Layout Thread**: High-frequency event processing, layout computation
//! - **Render Thread**: 60 FPS GPU command submission, VSync synchronized
//! - **Model Thread**: Application state management (external to Spill)
//!
//! ### Double Buffer System
//!
//! The heart of Spill's performance is a lock-free double buffer for paintables:
//!
//! ```text
//! ┌─────────────┐     Atomic Swap     ┌─────────────┐
//! │ Back Buffer │ <─────────────────→ │Front Buffer │
//! │   (Write)   │      (lock-free)    │   (Read)    │
//! └─────────────┘                     └─────────────┘
//!       ↑                                     ↓
//!  Layout Thread                        Render Thread
//! ```
//!
//! See [`paintable::double_buffer`] for implementation details.
//!
//! ## Component System
//!
//! Spill provides React-like stateful components:
//!
//! ```rust
//! use spill::component::{StatefulComponent, ComponentManager};
//! use spill::view::View;
//!
//! struct Counter { count: i32 }
//!
//! impl StatefulComponent for Counter {
//!     type State = i32;
//!     type Event = ();
//!
//!     fn update_views(&self, state: &Self::State, ctx: &mut RenderContext) -> View {
//!         // Build UI from state
//!         todo!()
//!     }
//!
//!     fn update(&self, event: Self::Event, state: &Self::State) -> Self::State {
//!         state + 1  // Immutable state updates
//!     }
//!
//!     fn initial_state(&self) -> Self::State { 0 }
//! }
//! ```
//!
//! See [`component`] module for complete component system documentation.
//!
//! ## Event Flow
//!
//! Events flow through the system with two classification types:
//!
//! - **Lens Events**: Layout-only changes (sidebar resize, drag handles)
//! - **Impulse Events**: Model state changes (button clicks, text input)
//!
//! ```text
//! OS Input → UiEvent → Hit Test → Event Classification
//!                                        ↓
//!                          ┌─────────────┴────────────┐
//!                          ↓                          ↓
//!                    Lens Event                 Impulse Event
//!                  (immediate)                  (via transaction)
//!                          ↓                          ↓
//!                   Component Update           Model Update
//!                          ↓                          ↓
//!                   View Regeneration          Transaction
//!                          ↓                          ↓
//!                   Paintable Generation       State Snapshot
//! ```
//!
//! See [`interaction`] module for event system details.
//!
//! ## Performance Characteristics
//!
//! - **Idle**: Zero layout, sleeping (no CPU usage)
//! - **Single events**: One layout pass per batch (~0.1-1ms)
//! - **Continuous drag**: Layout every frame (~0.1-1ms per frame)
//! - **Animations**: Controlled frame rate (30-60fps)
//! - **Render**: 60 FPS fixed (~1ms per frame for typical UI)
//!
//! ## Module Organization
//!
//! - [`component`] - Stateful component system with React-like patterns
//! - [`view`] - Declarative UI tree descriptions
//! - [`paintable`] - Low-level rendering primitives and double buffer
//! - [`render`] - GPU rendering pipeline (wgpu)
//! - [`interaction`] - Event handling and input processing
//! - [`layout`] - Taffy-based CSS layout (flexbox, grid)
//! - [`physics`] - Spring-based momentum and animations
//! - [`transaction`] - Lock-free cross-thread communication
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use spill::component::{ComponentManager, examples::HolyGrailV2Component};
//! use spill::window::WindowConfig;
//!
//! let component = HolyGrailV2Component::default();
//! let mut manager = ComponentManager::new();
//! manager.register_root(component);
//!
//! // See examples/ and tests/ for complete applications
//! ```

// Documentation lints - warn on missing docs for public APIs
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
#![warn(rustdoc::private_intra_doc_links)]

pub mod audio;
pub mod audio_source;
pub mod automap;

// New semantically organized modules
pub mod interaction;
pub mod layout;
pub mod paintable;
pub mod physics;
pub mod types;

// Higher-level modules
pub mod component;
pub mod font;
pub mod render;
pub mod transaction;
pub mod ux_thread;
pub mod view;

// Window module only for desktop/winit builds
#[cfg(feature = "winit")]
pub mod window;

// Test harness for headless component testing
// #[cfg(any(test, feature = "test-utils"))]
// pub mod test_harness;

/// Prelude module with commonly used types and re-exports
///
/// Import this module to get convenient access to external dependencies
/// and frequently used Spill types:
///
/// ```rust
/// use spill::prelude::*;
///
/// // Now have access to:
/// // - na (nalgebra) for vectors/matrices
/// // - taffy for layout types (Style, Size, etc.)
/// // - wgpu for GPU types
/// // - winit for window types (feature-gated)
/// // - GridBuilder for grid layouts
/// ```
pub mod prelude {
    pub use nalgebra as na;
    pub use taffy;
    // Re-export common external types
    pub use wgpu;
    #[cfg(feature = "winit")]
    pub use winit;

    pub use crate::component::GridBuilder;
    #[cfg(feature = "winit")]
    pub use crate::window::{EventClass, ModelState, WindowConfig};
}

/// Spill version string from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Common error types for the library
#[derive(Debug, thiserror::Error)]
pub(crate) enum SpillError {
    #[error("GPU initialization failed: {0}")]
    GpuInit(String),

    #[error("Audio bridge error: {0}")]
    Audio(String),

    #[error("Shader compilation failed: {0}")]
    Shader(String),

    #[error("Window creation failed: {0}")]
    Window(String),

    #[error("Windowing system error: {0}")]
    WindowingError(String),

    #[error("GPU resource error: {0}")]
    GpuError(String),
}

/// Convenience Result type with SpillError
pub type Result<T> = std::result::Result<T, SpillError>;
