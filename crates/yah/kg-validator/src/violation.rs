//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! Re-exports of the wire types — [`Severity`] and [`Violation`] live in
//! `yah-kg::validate` so the contract crate owns the shape that the daemon
//! and the UI both see. Engine code in this crate uses these names.

pub use kg::validate::{Severity, Violation};
