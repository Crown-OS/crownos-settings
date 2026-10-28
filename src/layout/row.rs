use crownui::kit::{INTER_FAMILY, TEXT_SIZE, icon, setting_row as kit_setting_row, use_theme};
use crownui::prelude::{
    Cx, IntoProp, Runtime, Styled, View, component, hstack, keyed, spacer, text, vstack,
};

use crate::util::theme::tint;

const ROW_PADDING_V: f32 = 14.0;
const ROW_PADDING_H: f32 = 18.0;
const ROW_DESC_SIZE: f32 = 12.0;
/// Gap between a row's title and its muted description line.
const ROW_DESC_GAP: f32 = 3.0;
/// Gap between a row's leading icon and its title.
const ROW_ICON_GAP: f32 = 10.0;
/// Gap between a row's text and its control.
const ROW_CONTROL_GAP: f32 = 12.0;
const ROW_ICON_SIZE: f32 = 18.0;

/// A single setting row: title on the left, `control` right-aligned.
pub fn setting_row(title: impl Into<String>, control: impl View) -> impl View {
    kit_setting_row(title, control)
}

/// [`setting_row`] with a muted description line under the title.
pub fn setting_row_desc(
    title: impl IntoProp<String> + 'static,
    description: impl IntoProp<String> + 'static,
    control: impl View,
) -> impl View {
    component(move |cx| row_shell(titled(cx, title, description), control))
}

/// [`setting_row`] behind a leading icon — a [`glyph`] or [`live_glyph`].
pub fn setting_row_icon(
    leading: impl View,
    title: impl IntoProp<String> + 'static,
    control: impl View,
) -> impl View {
    component(move |cx| {
        let title = row_title(cx, title);
        row_shell(beside(leading, title), control)
    })
}

/// [`setting_row_icon`] with [`setting_row_desc`]'s muted line under the
/// title — a row that introduces a thing and says what state it is in.
pub fn setting_row_icon_desc(
    leading: impl View,
    title: impl IntoProp<String> + 'static,
    description: impl IntoProp<String> + 'static,
    control: impl View,
) -> impl View {
    component(move |cx| {
        let text = titled(cx, title, description);
        row_shell(beside(leading, text), control)
    })
}

/// A row whose whole interior the caller composes, wearing nothing but the
/// padding every other row wears: a dismissible error, a note, a quiet
/// "looking for networks…". The content keeps its natural width; one that
/// should reach the card's edge says so itself, with
/// [`spread`](crate::layout::spread).
pub fn setting_row_content(content: impl View) -> impl View {
    vstack(content)
        .items_start()
        .px(ROW_PADDING_H)
        .py(ROW_PADDING_V)
}

/// A row's leading icon, in the theme's icon color. An empty body still
/// reserves the column, so an unmarked row lines up with a marked one.
pub fn glyph(body: &'static str) -> impl View {
    icon(body).size(ROW_ICON_SIZE)
}

/// A [`glyph`] whose body follows state, rebuilt only when the glyph changes.
pub fn live_glyph(body: impl FnMut(&mut Runtime) -> &'static str + 'static) -> impl View {
    keyed(body, |_, body| glyph(body)).self_center()
}

fn row_shell(leading: impl View, control: impl View) -> impl View {
    hstack((leading, spacer(), control))
        .items_center()
        .gap(ROW_CONTROL_GAP)
        .px(ROW_PADDING_H)
        .py(ROW_PADDING_V)
        .font_family(INTER_FAMILY)
}

fn beside(leading: impl View, text: impl View) -> impl View {
    hstack((leading, text))
        .gap(ROW_ICON_GAP)
        .items_center()
        .shrink(1.0)
}

fn row_title<T: IntoProp<String>>(cx: &mut Cx, title: T) -> impl View + use<T> {
    let theme = use_theme(cx);
    text(title)
        .text_size(TEXT_SIZE)
        .text_color(tint(theme, |theme| theme.text.body))
}

fn titled<T, D>(cx: &mut Cx, title: T, description: D) -> impl View + use<T, D>
where
    T: IntoProp<String>,
    D: IntoProp<String>,
{
    let theme = use_theme(cx);
    let title = row_title(cx, title);
    vstack((
        title,
        text(description)
            .text_size(ROW_DESC_SIZE)
            .text_color(tint(theme, |theme| theme.text.muted)),
    ))
    .gap(ROW_DESC_GAP)
    .shrink(1.0)
}
