//! Anti-library — read plain text files like a book.
//!
//! [`text`] turns a file into paragraphs and chapters, [`library`] remembers
//! where the reader stopped, and [`gui`] is the desktop reader. The terminal
//! reader lives in `src/bin/antilib.rs`.

pub mod crash;
pub mod diagnostics;
pub mod gui;
pub mod import;
pub mod qa;
pub mod library;
pub mod text;
