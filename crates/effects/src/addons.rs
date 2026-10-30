//! Effects of VectorCraft's built-in add-ons, one module per add-on.
//!
//! - [`color`]: Color Effects' live Exposure, Desaturate and Invert Colors, colour adjustments
//!   evaluated with the others ([`crate::adjust`]).
//! - [`duotone`]: Color Effects' live Duotone, a colour adjustment that prints each colour's tone
//!   with one to four inks.
//! - [`halftone`]: Color Effects' live Vector Halftone, and the screens Object › Vector Halftone
//!   builds its dots with.

pub mod color;
pub mod duotone;
pub mod halftone;
