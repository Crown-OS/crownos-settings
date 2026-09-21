//! The sidebar's menu column, built from the page registry.
//!
//! This module knows how the sidebar *looks*; [`crate::pages::GROUPS`] knows
//! what goes in it. Nothing here names an individual page, so a new page appears
//! in the sidebar as soon as it's registered.
//!
//! Only the rows live here. The sidebar chrome — the fixed column, its fill,
//! and the rounded content pane beside it — is
//! [`crownuikit::layouts::sidebar::sidebar`], which [`crate::app::root_view`]
//! hands this column to.

use crownuikit::layouts::sidebar::{sidebar_item, sidebar_section};
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{flex_col, CrossAxisAlignment};
use xilem::WidgetView;

use crate::app::AppState;
use crate::pages::{self, PageDescriptor, PageGroup};

const GROUP_GAP: f64 = 6.0;

/// The brand row plus one labelled section per registry group.
pub fn sidebar_menus(current: &'static PageDescriptor) -> impl WidgetView<AppState> {
    let sections: Vec<_> = pages::GROUPS
        .iter()
        .map(|group| group_view(group, current))
        .collect();

    flex_col(sections)
        .gap(GROUP_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Fill)
}

fn group_view(
    group: &'static PageGroup,
    current: &'static PageDescriptor,
) -> impl WidgetView<AppState> {
    let items: Vec<_> = group
        .pages
        .iter()
        .map(|page| {
            // Both sides come out of `GROUPS`, so identity is the selection
            // test — no need for the pages to carry a comparable id.
            let selected = std::ptr::eq(page, current);
            sidebar_item(
                page.title,
                page.icon,
                selected,
                move |state: &mut AppState| {
                    state.show(page);
                },
            )
        })
        .collect();

    sidebar_section(group.label, items)
}
