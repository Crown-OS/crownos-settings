//! When the screen locks by itself.

use crownos_config::schema::lock::{IdleLockAfterSecs, LockOnSuspend};
use crownui::prelude::{Cx, View};

use crate::controls::{Options, choice, switch};
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::state::Store;

static IDLE_DELAYS: Options<u32> = Options::new(&[
    ("Never", 0),
    ("1 minute", 60),
    ("2 minutes", 120),
    ("5 minutes", 300),
    ("10 minutes", 600),
    ("15 minutes", 900),
    ("30 minutes", 1800),
]);

pub fn behaviour_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "Behaviour",
        (
            setting_row_desc(
                "Lock after",
                "How long the computer sits idle before it locks",
                choice(cx, store, IdleLockAfterSecs, &IDLE_DELAYS),
            ),
            settings_divider(),
            setting_row("Lock before sleeping", switch(cx, store, LockOnSuspend)),
        ),
    )
}
