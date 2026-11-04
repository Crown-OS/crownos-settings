//! The whole window, driven headlessly: no config files, no NetworkManager,
//! no daemons — a defaults store, optionally populated as if every backend had
//! spoken.

use crowntest::{Role, TestApp, role, text};
use crownui::prelude::component;

use super::fixtures::populate;
use super::root::shell;
use crate::pages;
use crate::state::Store;

const WIDTH: f32 = 1040.0;
const HEIGHT: f32 = 720.0;

fn window(ordinal: usize, live: bool) -> TestApp {
    TestApp::with_size(
        component(move |cx| {
            let store = Store::defaults(cx);
            if live {
                populate(cx, store);
            }
            shell(cx, store, ordinal)
        }),
        WIDTH,
        HEIGHT,
    )
}

fn ordinal_of(title: &str) -> usize {
    pages::all_pages()
        .position(|page| page.title == title)
        .unwrap_or_else(|| panic!("{title} is registered"))
}

/// Each page has to assemble in the state it shows before any backend has
/// spoken, and open the way a user opens it.
#[test]
fn every_page_opens_from_the_sidebar() {
    let mut app = window(pages::DEFAULT_PAGE, false);
    for page in pages::all_pages() {
        app.click(role(Role::MenuItem).name(page.title));
        app.settle();
        assert!(
            app.exists(role(Role::ScrollView).name(page.title)),
            "{} did not open:\n{}",
            page.title,
            app.semantics_snapshot()
        );
    }
}

#[test]
fn every_page_builds_with_every_backend_populated() {
    for page in pages::all_pages() {
        let mut app = window(ordinal_of(page.title), true);
        app.settle();
        assert!(app.exists(role(Role::ScrollView).name(page.title)));
    }
}

/// A secured network grows a password field in its own row, and Join does
/// not arm until the key is one NetworkManager would accept.
#[test]
fn joining_a_secured_network_asks_for_a_password_first() {
    let mut app = window(ordinal_of("Wi-Fi"), true);
    let field = || role(Role::TextField).name("Password for Café");
    assert!(!app.exists(field()));

    app.click(role(Role::Button).name("Join Café"));
    assert!(app.exists(field()), "the prompt opens in the row");
    assert!(
        !app.exists(role(Role::Button).name("Join Café")),
        "the row's own Join steps aside"
    );
    assert!(app.find(role(Role::Button).name("Join")).states.disabled);

    // Secure fields refuse agent typing, so this types the way a person does.
    app.click(field());
    press(&mut app, "hunter");
    assert!(
        app.find(role(Role::Button).name("Join")).states.disabled,
        "six bytes is too short"
    );
    press(&mut app, "22");
    assert!(!app.find(role(Role::Button).name("Join")).states.disabled);

    app.click(role(Role::Button).name("Join"));
    assert!(!app.exists(field()), "submitting closes the prompt");
}

#[test]
fn searching_opens_the_picked_page_and_clears_the_bar() {
    let mut app = window(pages::DEFAULT_PAGE, false);
    let bar = || role(Role::SearchField).name("Search settings");
    app.click(bar());
    press(&mut app, "lock");
    app.settle();
    app.click(role(Role::ListItem).name("Lock Screen"));
    app.settle();
    assert!(app.exists(role(Role::ScrollView).name("Lock Screen")));
    assert_eq!(app.find(bar()).value.as_deref(), Some(""));
}

#[test]
fn enter_opens_the_first_match() {
    let mut app = window(pages::DEFAULT_PAGE, false);
    app.click(role(Role::SearchField));
    press(&mut app, "battery");
    app.press_key("Enter");
    app.settle();
    assert!(app.exists(role(Role::ScrollView).name("Battery & Power")));
}

#[test]
fn a_keyword_finds_the_page_that_holds_it() {
    let mut app = window(pages::DEFAULT_PAGE, false);
    app.click(role(Role::SearchField));
    press(&mut app, "noise");
    app.press_key("Enter");
    app.settle();
    assert!(app.exists(role(Role::ScrollView).name("Voice")));
}

fn press(app: &mut TestApp, keys: &str) {
    for key in keys.chars() {
        app.press_key(&key.to_string());
    }
}

#[test]
fn details_expand_and_collapse_inline() {
    let mut app = window(ordinal_of("Wi-Fi"), true);
    assert!(!app.exists(role(Role::Button).name("Disconnect")));
    app.click(role(Role::Button).name("Details…"));
    assert!(app.exists(role(Role::Button).name("Disconnect")));
    app.click(role(Role::Button).name("Hide Details"));
    assert!(!app.exists(role(Role::Button).name("Disconnect")));
}

/// Thumbnails are GPU memory spent on a grid the user may rarely look at:
/// leaving the picker hands every one back to the renderer.
#[test]
fn leaving_the_picker_frees_every_thumbnail() {
    let mut app = window(ordinal_of("Appearance"), true);
    app.settle();
    let uploaded = app.inline().renderer().image_count();
    assert!(uploaded > 0, "the gallery's thumbnails are on the GPU");

    app.click(role(Role::MenuItem).name("Display"));
    app.settle();
    assert_eq!(app.inline().renderer().image_count(), 0);
}

/// An enabled plugin that declares settings grows a sidebar row of its own,
/// and its page draws every declared field.
#[test]
fn plugin_settings_get_their_own_sidebar_page() {
    let mut app = window(pages::DEFAULT_PAGE, false);
    assert!(!app.exists(role(Role::MenuItem).name("Weather")));

    let mut app = window(pages::DEFAULT_PAGE, true);
    app.settle();
    app.click(role(Role::MenuItem).name("Weather"));
    app.settle();
    assert!(
        app.exists(role(Role::ScrollView).name("Weather")),
        "the plugin page did not open:\n{}",
        app.semantics_snapshot()
    );
    assert!(app.exists(role(Role::Switch).name("Use Celsius")));
    assert!(app.exists(role(Role::Slider).name("Refresh every")));
    assert!(
        app.exists(text("30 min")),
        "the stored value, not the default"
    );
    assert!(app.exists(role(Role::ComboBox).name("Icon color")));
    assert!(app.exists(role(Role::TextField).name("API key")));
    assert!(app.exists(role(Role::TextField).name("Icon pack")));
}

/// `--plugin <id>` opens the Plugins & Themes page on that entity.
#[test]
fn a_plugin_detail_groups_its_capabilities_by_level() {
    let mut app = TestApp::with_size(
        component(|cx| {
            let store = Store::defaults(cx);
            store
                .plugins
                .update(cx, |plugins| plugins.open("broken".to_owned()));
            populate(cx, store);
            shell(cx, store, pages::plugins_ordinal())
        }),
        WIDTH,
        HEIGHT,
    );
    app.settle();
    assert!(
        app.exists(role(Role::Button).name("All Plugins & Themes")),
        "{}",
        app.semantics_snapshot()
    );
    assert!(app.exists(text("panicked three times")));
    assert!(app.exists(role(Role::Group).name("Permission for Use net")));
    assert!(app.exists(role(Role::Group).name("Permission for Use exec")));
    assert!(
        !app.exists(role(Role::Group).name("Permission for Use timer")),
        "a safe capability is granted, not asked"
    );
    assert!(app.exists(text("attempt to index a nil value")));

    app.click(role(Role::Button).name("All Plugins & Themes"));
    app.settle();
    assert!(app.exists(role(Role::Switch).name("Enable Weather")));
    assert!(app.exists(role(Role::Button).name("Use Stock look")));
}

#[test]
fn a_missing_plugin_daemon_reads_as_a_calm_notice() {
    let mut app = TestApp::with_size(
        component(|cx| {
            let store = Store::defaults(cx);
            store.plugins.update(cx, |plugins| {
                plugins.link = crate::net::plugins::Link::Offline
            });
            shell(cx, store, pages::plugins_ordinal())
        }),
        WIDTH,
        HEIGHT,
    );
    app.settle();
    assert!(app.exists(text("Plugins aren't running")));
}

/// The enhancement belongs to the system-wide microphone, so it waits for
/// that to be switched on; the hardware choice applies to every stream.
#[test]
fn the_enhancement_waits_for_the_all_apps_switch() {
    let mut app = window(ordinal_of("Voice"), false);
    app.settle();
    assert_eq!(app.find(role(Role::Switch)).states.checked, Some(false));
    assert!(
        app.find(role(Role::ComboBox).name("Noise suppression"))
            .states
            .disabled
    );
    assert!(
        !app.find(role(Role::ComboBox).name("Automatic"))
            .states
            .disabled
    );
    assert!(app.exists(text("Checking…")));
}

#[test]
fn the_voice_status_names_the_backend_or_stays_calm_without_one() {
    let mut app = window(ordinal_of("Voice"), true);
    app.settle();
    assert!(app.exists(text("cpu: Ryzen 7 7840U")));

    let mut app = TestApp::with_size(
        component(|cx| {
            let store = Store::defaults(cx);
            store.voice.update(cx, |voice| {
                voice.service = crate::net::voice::VoiceService::NotInstalled;
            });
            shell(cx, store, ordinal_of("Voice"))
        }),
        WIDTH,
        HEIGHT,
    );
    app.settle();
    assert!(app.exists(text("Not installed")));
    assert!(!app.exists(role(Role::Dialog)));
}

#[test]
fn the_microphone_picker_lists_pipewire_sources_by_description() {
    let mut app = window(ordinal_of("Input"), true);
    app.settle();
    assert!(
        app.exists(role(Role::ComboBox).name("Automatic (Built-in Audio)")),
        "{}",
        app.semantics_snapshot()
    );
}

/// Renders every page through Impeller's offscreen GLES target into
/// `$CROWNSETTINGS_RENDER_DIR/<ordinal>.png` (and, with every backend
/// populated, `<ordinal>-live.png`), for looking at.
#[test]
#[ignore = "needs a GPU; writes PNGs for manual inspection"]
fn render_every_page() {
    use crownui::ext::{InlineApp, TextSystem};
    use crownui::kit::{INTER_FAMILY, inter_font};
    use crownui::prelude::Size;
    use impeller::{ImpellerBackend, Offscreen};
    use taffy_layout::TaffyEngine;

    let Some(dir) = std::env::var_os("CROWNSETTINGS_RENDER_DIR") else {
        return;
    };
    let size = Size::new(WIDTH, 1400.0);
    let only = std::env::var("CROWNSETTINGS_RENDER_PAGE")
        .ok()
        .map(|title| ordinal_of(&title));
    let shots = (0..pages::all_pages().count())
        .filter(|ordinal| only.is_none_or(|only| only == *ordinal))
        .flat_map(|ordinal| [(ordinal, false), (ordinal, true)]);
    for (ordinal, live) in shots {
        let surface = Offscreen {
            width: size.width as u32,
            height: size.height as u32,
            scale: 1.0,
        };
        let root = component(move |cx| {
            let store = Store::defaults(cx);
            if live {
                populate(cx, store);
                store.wifi.update(cx, |wifi| {
                    wifi.prompt = Some("Café".to_owned());
                    wifi.details_open = true;
                });
            }
            shell(cx, store, ordinal)
        });
        let mut app = InlineApp::<ImpellerBackend, TaffyEngine>::with_surface(
            root,
            TaffyEngine::new(),
            size,
            1.0,
            &surface,
        )
        .expect("an offscreen renderer");
        app.text_cache()
            .system_mut()
            .register_font(inter_font().to_vec(), Some(INTER_FAMILY))
            .expect("Inter registers");
        app.settle().expect("a frame renders");
        let pixels = app.renderer_mut().read_pixels().expect("pixels read back");
        let png = impeller::encode_png(&pixels).expect("PNG encodes");
        let name = if live {
            format!("{ordinal}-live.png")
        } else {
            format!("{ordinal}.png")
        };
        std::fs::write(std::path::Path::new(&dir).join(name), png).expect("PNG written");
    }
}
