//! Internal wire-safety predicates shared across the crate.
//!
//! ESL is a line-delimited text protocol; embedded `\n`/`\r` in user-supplied
//! strings would inject arbitrary commands. Each call site keeps its own
//! error type and message — this module only provides the predicate.
//!
//! This module is `#[doc(hidden)]` and not part of the stable API surface.
//! It is exposed publicly only so the `freeswitch-esl-tokio` crate (same
//! workspace) can re-export the predicate as `pub(crate)` without depending
//! on its own copy. Do not rely on it from external crates.

/// `true` if `s` contains either `\n` or `\r`, the two ESL line terminators
/// that would let user input split into a new wire command.
#[doc(hidden)]
pub fn contains_wire_terminator(s: &str) -> bool {
    s.contains('\n') || s.contains('\r')
}
