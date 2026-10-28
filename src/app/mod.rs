//! The root view, and the only places that know a page can be switched.

#[cfg(test)]
mod fixtures;
mod navigation;
mod root;
#[cfg(test)]
mod tests;
mod theme;

pub use navigation::Navigation;
pub use root::root;
