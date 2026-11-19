//! Every sidebar page once, as the sidebar opens it over the sample machine,
//! plus a plugin's own settings page from the sidebar's Plugins group.

use anyhow::Result;
use capture::Carrier;
use crowntest::{Role, role};

use super::open_page;
use crate::fixtures::populate_all;
use crate::net::memory::{Memory, SwapPartition};
use crate::pages;
use crate::session::Window;

const GROUP: &str = "Sidebar pages";

pub const SAMPLE_MEMORY: Memory = Memory {
    ram_mib: 32_768,
    zram_percent: Some(50),
    swap: Some(SwapPartition {
        gib: 16,
        max_gib: 96,
    }),
};

pub fn capture(carrier: &mut Carrier) -> Result<()> {
    for page in pages::all_pages() {
        let mut window = open_page(page.title, populate_all)?;
        carrier.capture_state(
            &mut window.app,
            GROUP,
            page.title,
            &format!("The {} page as the sidebar opens it", page.title),
        )?;
    }

    let mut window = Window::open(pages::DEFAULT_PAGE, populate_all)?;
    if window.click(role(Role::MenuItem).name("CPU Graph")) {
        carrier.capture_state(
            &mut window.app,
            GROUP,
            "Plugin settings page",
            "A plugin's own settings page, grown in the sidebar's Plugins group",
        )?;
    }
    Ok(())
}
