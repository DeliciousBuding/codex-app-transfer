//! Anthropic Messages adapter building blocks.
//!
//! P3 lands request-side lowering and P4 lands response-side stream conversion.
//! Adapter/registry wiring is tracked by P5 so this module can be tested
//! without exposing a half-complete provider entry point.

pub mod request;
pub mod response;
