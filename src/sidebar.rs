//! The fixed left sidebar, built from the page registry.
//!
//! This module knows how the sidebar *looks*; [`crate::pages::GROUPS`] knows
//! what goes in it. Nothing here names an individual page, so a new page appears
//! in the sidebar as soon as it's registered.

use blinc_icons::icons;
use crownuikit::layouts::sidebar::{sidebar, sidebar_brand, sidebar_item, sidebar_separator};
use xilem::WidgetView;
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{AnyFlexChild, CrossAxisAlignment, FlexExt, FlexSpacer, flex_col};

use crate::app::AppState;
use crate::pages::{self, PageDescriptor};

/// Gap between the brand, each group, and each separator.
const GROUP_GAP: f64 = 6.0;
/// Gap between items within one group — tighter, so a group reads as a unit.
const ITEM_GAP: f64 = 2.0;

/// The sidebar, with `current` highlighted.
///
/// Groups and separators have different view types, so the children are
/// collected as [`AnyFlexChild`] — a `Vec` of those is a flex sequence, which is
/// what lets the list be built by iterating the registry instead of being
/// spelled out.
pub fn sidebar_view(current: &'static PageDescriptor) -> impl WidgetView<AppState> {
    let mut children: Vec<AnyFlexChild<AppState>> = Vec::new();

    children.push(
        sidebar_brand("CrownOS Settings", icons::CROWN, |_: &mut AppState| ()).into_any_flex(),
    );

    for (index, group) in pages::GROUPS.iter().enumerate() {
        if index > 0 {
            children.push(sidebar_separator().into_any_flex());
        }
        children.push(group_view(group, current).into_any_flex());
    }

    // Keeps the groups pinned to the top of a taller-than-content sidebar.
    children.push(AnyFlexChild::Spacer(FlexSpacer::Flex(1.0)));

    let content = flex_col(children)
        .gap(GROUP_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start);

    sidebar(content)
}

/// One separator-delimited run of sidebar entries.
fn group_view(
    group: &'static [PageDescriptor],
    current: &'static PageDescriptor,
) -> impl WidgetView<AppState> {
    let items: Vec<_> = group
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

    flex_col(items).gap(ITEM_GAP.px())
}
