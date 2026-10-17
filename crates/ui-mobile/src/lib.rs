//! Touch-first EffectCraft frontend model (layer 5).
//!
//! This crate holds the parts of the mobile / tablet interface that are pure logic and so can be
//! tested without a window: the design tokens ([`tokens`]), the responsive workspace layout
//! ([`layout`]) and the routing of external-keyboard shortcuts through the engine's shortcut table
//! ([`keys`]), plus a typed facade over the engine session ([`editor`]). The egui rendering that consumes them lives on top of this.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod editor;
pub mod keys;
pub mod layout;
pub mod ops;
pub mod tokens;
pub mod ui;
