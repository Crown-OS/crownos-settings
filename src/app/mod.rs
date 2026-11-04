//! The root view, and the only places that know a page can be switched.

#[cfg(test)]
mod fixtures;
mod navigation;
#[cfg(test)]
mod profile;
mod root;
mod search;
#[cfg(test)]
mod tests;
mod theme;

pub use navigation::Navigation;
pub use root::root;
