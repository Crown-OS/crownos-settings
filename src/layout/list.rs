use crownui::prelude::{Signal, View, bind, show};

use super::{nothing, settings_divider};

/// One row of a card built from a list, knowing whether a hairline goes above
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed<T> {
    pub first: bool,
    pub item: T,
}

/// `items` ready for a keyed `for_each`: a hairline between every pair of
/// rows and nowhere else.
pub fn listed<T>(items: impl IntoIterator<Item = T>) -> Vec<Listed<T>> {
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| Listed {
            first: index == 0,
            item,
        })
        .collect()
}

/// The hairline a [`Listed`] row wears above itself, unless it leads the card.
pub fn hairline_unless_first<T: 'static>(row: Signal<Listed<T>>) -> impl View {
    show(
        bind(move |runtime| row.with(runtime, |row| !row.first)),
        settings_divider,
        nothing,
    )
}
