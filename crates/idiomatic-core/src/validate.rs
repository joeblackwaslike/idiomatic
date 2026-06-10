//! Load-time validation of resolved idioms.
use crate::error::ResolveError;
use crate::resolve::Idiom;

pub fn check_invariants(_idiom: &Idiom) -> Result<(), ResolveError> {
    Ok(())
}
