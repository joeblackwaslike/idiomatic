//! Core of the `idiomatic` idiom-enforcement framework.

pub mod error;
pub mod pack;
pub mod resolve;
pub mod validate;
pub mod engine;
pub mod selftest;
pub mod render;
pub mod telemetry;

/// A configuration layer, lowest to highest precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Base,
    User,
    Project,
}

/// The seed packs shipped with the binary (the cascade's `base` layer).
pub fn builtin_packs() -> &'static [(&'static str, &'static str)] {
    &[("python-core", include_str!("../packs/python-core.yaml"))]
}
