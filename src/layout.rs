//! # Settings-page layout
use crownuikit::config::theme;
use crownuikit::util::{scale_alpha, INTER};
use crownuikit::widgets::{card, divider, icon, Divider};
use xilem::masonry::core::ArcStr;
use xilem::masonry::properties::types::AsUnit;
use xilem::masonry::properties::Padding;
use xilem::style::Style;
use xilem::view::{flex_col, flex_row, label, portal, sized_box, CrossAxisAlignment, FlexSpacer};
use xilem::{FontWeight, WidgetView};

// --- MARK: Page chrome constants ---
//
// Only metrics live here. Every color comes from `crownuikit::config::theme`,
// which is what makes this app follow `~/.config/crownos/appearance.ron` — see
// the note on colors at the bottom of this section.

/// Padding around the whole scrollable page.
const PAGE_PADDING: f64 = 32.0;
/// Gap between the page header and the first card.
const PAGE_TITLE_GAP: f64 = 24.0;
const PAGE_TITLE_SIZE: f32 = 22.0;
/// Gap between the header's icon chip and the title.
const PAGE_ICON_GAP: f64 = 12.0;
/// The chip's own footprint — a rounded square, not a circle, matching the
/// kit's other +2px-rounder corners rather than a pill.
const PAGE_ICON_CHIP_SIZE: f64 = 34.0;
const PAGE_ICON_CHIP_RADIUS: f64 = 10.0;
const PAGE_ICON_SIZE: f64 = 18.0;
/// How much of the accent's own alpha the chip's background tint carries —
/// the same soft-wash convention `crownuikit::widgets::badge` tints its pill
/// with, so a page header and a badge chip read as the same family of thing.
const PAGE_ICON_TINT_ALPHA: f64 = 0.14;

// --- MARK: Card chrome constants ---

/// Gap between a titled card's section label and the card itself.
const SECTION_LABEL_GAP: f64 = 8.0;
const SECTION_LABEL_SIZE: f32 = 12.0;

// --- MARK: Row constants ---

const ROW_PADDING_V: f64 = 14.0;
const ROW_PADDING_H: f64 = 18.0;
const ROW_TITLE_SIZE: f32 = 14.0;
const ROW_DESC_SIZE: f32 = 12.0;
/// Gap between a row's title and its muted description line.
const ROW_DESC_GAP: f64 = 3.0;
/// Gap between a row's leading icon and its title.
const ROW_ICON_GAP: f64 = 10.0;
const ROW_ICON_SIZE: f64 = 18.0;

// --- MARK: Page ---

/// A rounded, softly-tinted square holding one icon — the small mark every
/// page header wears in front of its title, in the accent's own color at a
/// fraction of its strength.
///
/// Padding rather than a fixed width/height plus alignment: `sized_box` places
/// its child at its own top-left corner, not centered, so the way to land an
/// 18px icon in the middle of a 34px chip is to give it (34 - 18) / 2 on every
/// side and let the box's size fall out of that.
fn page_icon_chip<State: 'static, Action: 'static>(
    icon_svg: &'static str,
) -> impl WidgetView<State, Action> {
    let theme = theme();
    let pad = (PAGE_ICON_CHIP_SIZE - PAGE_ICON_SIZE) / 2.0;
    sized_box(icon(icon_svg).size(PAGE_ICON_SIZE).color(theme.accent.end))
        .padding(Padding::all(pad))
        .background_color(scale_alpha(theme.accent.end, PAGE_ICON_TINT_ALPHA))
        .corner_radius(PAGE_ICON_CHIP_RADIUS)
}

/// Wraps a pre-composed content view in the settings-page chrome: an icon
/// chip and title heading the pane, generous padding, and a scrollable body
/// that expands to fill whatever width its parent gives it (pair with
/// [`FlexExt::flex`](xilem::view::FlexExt::flex) in the surrounding
/// `flex_row` so it actually claims the remaining space next to the
/// sidebar).
///
/// ```ignore
/// settings_page("Network", icons::WIFI, flex_col((
///     settings_card(( ... )),
///     settings_card(( ... )),
/// )).gap(24.0.px()))
/// ```
pub fn settings_page<State, Action, V>(
    title: impl Into<ArcStr>,
    icon_svg: &'static str,
    content: V,
) -> impl WidgetView<State, Action>
where
    // `xilem::view::Portal` stores `PhantomData<(State, Action)>` directly
    // (not the `fn() -> (State, Action)` trick most other views use), so
    // its `Send + Sync` auto-impl — required by `WidgetView`'s supertrait
    // bound — needs `State`/`Action` themselves to be `Send + Sync`. Any
    // ordinary app-state struct satisfies this already.
    State: 'static + Send + Sync,
    Action: 'static + Send + Sync,
    V: WidgetView<State, Action>,
{
    let header = flex_row((
        page_icon_chip(icon_svg),
        label(title.into())
            .text_size(PAGE_TITLE_SIZE)
            .weight(FontWeight::BOLD)
            .font(INTER)
            .color(theme().text.primary),
    ))
    .gap(PAGE_ICON_GAP.px())
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let body = flex_col((header, content))
        .gap(PAGE_TITLE_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start);

    // Padding lives inside the scroll region so it travels with the
    // content; `expand_width` (inherent — called before the Style-trait
    // `background_color`/`padding`, same ordering rule as the kit's
    // `sidebar`) keeps the body from shrinking to its natural width once
    // it's inside the loosened constraints `portal` hands its child.
    let padded = sized_box(body)
        .expand_width()
        .padding(Padding::all(PAGE_PADDING));

    // The outer box always claims the full pane (both axes), so `portal`
    // only kicks in a scrollbar once the body is taller than that fixed
    // pane. Deliberately no background: the pane's fill (with its rounded
    // corners and border) is painted by `crownuikit`'s `sidebar` layout,
    // and a square fill here would overpaint those corners. Pages slide
    // edge-to-edge, never overlapping, so they don't need their own
    // backdrop to occlude each other mid-transition.
    sized_box(portal(padded)).expand()
}

// --- MARK: Card ---

/// A grouped card of setting rows — the kit's [`card`](fn@card) chrome
/// around a zero-gap column. Takes any `FlexSequence` — typically a tuple of
/// [`setting_row`] (and friends) calls with [`settings_divider`] between
/// them, mirroring how `crownuikit::layouts::sidebar::sidebar_group`
/// expects callers to place `sidebar_separator` explicitly.
///
/// ```ignore
/// settings_card((
///     setting_row("Wi-Fi", toggle("", wifi, on_wifi)),
///     settings_divider(),
///     setting_row("Bluetooth", toggle("", bt, on_bt)),
/// ))
/// ```
pub fn settings_card<State, Action, Seq>(children: Seq) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: xilem::view::FlexSequence<State, Action> + Send + Sync + 'static,
{
    card(flex_col(children).gap(0.0.px()))
}

/// [`settings_card`] with a small uppercase, muted section label above it —
/// the "General", "Network", … headers macOS System Settings prints above
/// each card.
pub fn settings_card_titled<State, Action, Seq, L>(
    title: L,
    children: Seq,
) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    Seq: xilem::view::FlexSequence<State, Action> + Send + Sync + 'static,
    L: Into<ArcStr>,
{
    let heading = label(title.into().to_uppercase())
        .text_size(SECTION_LABEL_SIZE)
        .weight(FontWeight::SEMI_BOLD)
        .font(INTER)
        .color(theme().text.muted);

    flex_col((heading, settings_card(children)))
        .gap(SECTION_LABEL_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start)
}

// --- MARK: Rows ---

/// A row's title, in the theme's body voice. Named once because all three row
/// variants print the same thing, and because masonry's default `ContentColor`
/// would be wrong on at least one of the two palettes.
fn row_title<State: 'static, Action: 'static>(
    title: impl Into<ArcStr>,
) -> impl WidgetView<State, Action> {
    label(title.into())
        .text_size(ROW_TITLE_SIZE)
        .font(INTER)
        .color(theme().text.body)
}

/// Builds the shared row shell: `left` content, a flexible spacer, then
/// `control` right-aligned, all padded consistently.
fn row_shell<State, Action, L, V>(left: L, control: V) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    L: WidgetView<State, Action>,
    V: WidgetView<State, Action>,
{
    let row = flex_row((left, FlexSpacer::Flex(1.0), control))
        .cross_axis_alignment(CrossAxisAlignment::Center);
    sized_box(row).padding(Padding::from_vh(ROW_PADDING_V, ROW_PADDING_H))
}

/// A single setting row: title on the left, `control` right-aligned.
///
/// ```ignore
/// setting_row("Wi-Fi", toggle("", state.wifi, |s: &mut State, v| s.wifi = v))
/// ```
pub fn setting_row<State, Action, V>(
    title: impl Into<ArcStr>,
    control: V,
) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    let title = row_title(title);
    row_shell(title, control)
}

/// [`setting_row`] with a muted description line under the title.
///
/// ```ignore
/// setting_row_desc(
///     "Bluetooth",
///     "Discoverable by nearby devices",
///     toggle("", state.bluetooth, |s: &mut State, v| s.bluetooth = v),
/// )
/// ```
pub fn setting_row_desc<State, Action, V>(
    title: impl Into<ArcStr>,
    description: impl Into<ArcStr>,
    control: V,
) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    let left = flex_col((
        row_title(title),
        label(description.into())
            .text_size(ROW_DESC_SIZE)
            .font(INTER)
            .color(theme().text.muted),
    ))
    .gap(ROW_DESC_GAP.px())
    .cross_axis_alignment(CrossAxisAlignment::Start);
    row_shell(left, control)
}

/// [`setting_row`] with a leading icon (Lucide-style SVG body, same
/// convention as [`crownuikit::widgets::icon`](fn@icon) and the sidebar's
/// icon args).
///
/// ```ignore
/// setting_row_icon(icons::WIFI, "Wi-Fi", toggle("", state.wifi, |s, v| s.wifi = v))
/// ```
pub fn setting_row_icon<State, Action, V>(
    icon_svg: &'static str,
    title: impl Into<ArcStr>,
    control: V,
) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    let left = flex_row((
        icon(icon_svg).size(ROW_ICON_SIZE).color(theme().text.icon),
        row_title(title),
    ))
    .gap(ROW_ICON_GAP.px())
    .cross_axis_alignment(CrossAxisAlignment::Center);
    row_shell(left, control)
}

/// A row whose whole interior the caller composes, wearing nothing but the
/// padding every other row wears.
///
/// The three rows above all share one shape — a title on the left, a control on
/// the right — because that is what a *setting* looks like. A card
/// occasionally has to hold something that is not a setting: a dismissible
/// error line, a note explaining why the control above it is disabled, a quiet
/// "looking for networks…". Those want to line up with their neighbours and
/// want nothing else from this module, so this is the whole of what they get.
///
/// Note that `content` is laid out at its natural width. A row that should
/// reach the card's edge says so itself, with a
/// [`FlexSpacer::Flex`](xilem::view::FlexSpacer::Flex) inside — exactly the way
/// [`row_shell`] pushes a control to the right.
///
/// ```ignore
/// setting_row_content(flex_row((
///     status("Couldn't join Workshop").tone(Tone::Danger),
///     FlexSpacer::Flex(1.0),
///     button("Dismiss", dismiss).variant(ButtonVariant::Ghost).small(),
/// )))
/// ```
pub fn setting_row_content<State, Action, V>(content: V) -> impl WidgetView<State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    sized_box(content).padding(Padding::from_vh(ROW_PADDING_V, ROW_PADDING_H))
}

// --- MARK: Divider ---

/// Hairline divider for use between rows inside a [`settings_card`]: the
/// kit's [`divider`](fn@divider), unmodified. Full width, 1px, no extra
/// breathing room (the rows already carry their own vertical padding).
///
/// Kept as a named function rather than inlining `divider()` at each call
/// site, so a future page-specific hairline has one place to go.
pub fn settings_divider() -> Divider {
    divider()
}
