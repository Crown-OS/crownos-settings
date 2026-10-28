/// A `(label, value)` table for a setting with a fixed set of choices.
///
/// The single place a variant's user-facing name lives, and the translation
/// between those names and typed values — `select` speaks only in indices.
/// Declare one as a `static` next to the page that uses it.
pub struct Options<T: 'static>(&'static [(&'static str, T)]);

impl<T: 'static> Options<T> {
    /// Build a table. Entry order is the order the select lists them in.
    pub const fn new(entries: &'static [(&'static str, T)]) -> Self {
        Self(entries)
    }

    /// The labels, in table order.
    pub(super) fn labels(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|(label, _)| (*label).to_owned())
            .collect()
    }
}

impl<T: Copy + PartialEq + 'static> Options<T> {
    /// Where `value` sits in the table, or `None` when the table doesn't offer
    /// it — a RON file naming a variant this build doesn't have.
    ///
    /// `None` shows nothing selected. That is deliberately not "fall back to
    /// the first entry": that would misreport what is actually on disk.
    pub(super) fn index_of(&self, value: T) -> Option<usize> {
        self.0.iter().position(|(_, candidate)| *candidate == value)
    }

    /// The value at `index`, or `None` for an index the table doesn't have.
    pub(super) fn value_at(&self, index: usize) -> Option<T> {
        self.0.get(index).map(|(_, value)| *value)
    }

    /// The values, in table order.
    pub fn values(&self) -> impl Iterator<Item = T> + '_ {
        self.0.iter().map(|(_, value)| *value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SIZES: Options<u32> = Options::new(&[("Small", 1), ("Large", 4)]);

    #[test]
    fn values_and_indices_translate_both_ways() {
        assert_eq!(SIZES.labels(), ["Small", "Large"]);
        assert_eq!(SIZES.index_of(4), Some(1));
        assert_eq!(SIZES.index_of(2), None, "an unknown value selects nothing");
        assert_eq!(SIZES.value_at(0), Some(1));
        assert_eq!(SIZES.value_at(9), None);
    }
}
