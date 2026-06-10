//! Core of the `idiomatic` idiom-enforcement framework.

pub mod error;
pub mod pack;
pub mod resolve;
pub mod validate;
pub mod engine;
pub mod selftest;

/// A configuration layer, lowest to highest precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Base,
    User,
    Project,
}
