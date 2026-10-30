//! Effects of VectorCraft's built-in add-ons, one module per add-on.
//!
//! - [`color`]: Color Effects' live Exposure, Desaturate and Invert Colors, colour adjustments
//!   evaluated with the others ([`crate::adjust`]).
//! - [`halftone`]: Color Effects' live Vector Halftone, and the screens Object › Vector Halftone
//!   builds its dots with.

pub mod color;
pub mod halftone;
