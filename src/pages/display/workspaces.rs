//! Whether a workspace switch moves every monitor or only the focused one.
//! This is the only place the scope is switched.

use crownos_config::schema::{WorkspaceScope, compositor};
use crownui::prelude::{Cx, Semantic, View};

use crate::controls::{Options, choice};
use crate::layout::{setting_row_desc, settings_card_titled};
use crate::state::Store;

static SCOPES: Options<WorkspaceScope> = Options::new(&[
    ("Switch together on every monitor", WorkspaceScope::Shared),
    ("Each monitor on its own", WorkspaceScope::PerMonitor),
]);

pub fn card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "Workspaces",
        (setting_row_desc(
            "Workspaces",
            "What a workspace switch changes",
            choice(cx, store, compositor::Scope, &SCOPES).label("Workspaces"),
        ),),
    )
}
