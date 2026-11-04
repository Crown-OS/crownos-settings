//! Small pieces with no home in the layered stack — see [`crate`].

pub mod affine;
#[cfg(test)]
pub mod alloc_count;
pub mod bento;
pub mod bridge;
pub mod clock;
pub mod deadline;
pub mod follow_watch;
pub mod home;
pub mod jpeg;
pub mod lucide;
pub mod persist;
pub mod pixmap;
pub mod qr;
pub mod texture;
pub mod theme;
pub mod worker;
