//! pscat — a PostScript interpreter.
//!
//! See `ARCHITECTURE.md` at the repo root for the design writeup and
//! `INIT.md` for the project vision and staged roadmap.

pub mod encodings;
pub mod error;
pub mod file;
pub mod font;
pub mod gfx;
pub mod halftone;
pub(crate) mod image;
pub mod interp;
pub mod lexer;
pub mod name;
pub mod object;
pub mod ops;
pub mod pdf;
pub mod repl;
pub mod spool;
pub mod svg;
pub(crate) mod type1;
pub mod window;

pub use error::PsError;
pub use interp::Interp;
pub use object::{Object, Value};
