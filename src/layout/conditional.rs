use crownui::prelude::{Cx, Runtime, Styled, View, keyed, vstack};

/// A view that takes no space and no gap.
pub fn nothing() -> impl View {
    vstack(()).display_none()
}

/// `build(cx, value)` while `value` derives `Some`, [`nothing`] otherwise —
/// rebuilt only when the derived value changes.
pub fn optional<T, V>(
    value: impl FnMut(&mut Runtime) -> Option<T> + 'static,
    mut build: impl FnMut(&mut Cx, T) -> V + 'static,
) -> impl View
where
    T: Clone + PartialEq + 'static,
    V: View,
{
    keyed(value, move |cx, value| match value {
        Some(value) => build(cx, value).boxed(),
        None => nothing().boxed(),
    })
}

/// `build(cx)` while `condition` holds, [`nothing`] otherwise.
pub fn when<V: View>(
    mut condition: impl FnMut(&mut Runtime) -> bool + 'static,
    mut build: impl FnMut(&mut Cx) -> V + 'static,
) -> impl View {
    optional(
        move |runtime| condition(runtime).then_some(()),
        move |cx, ()| build(cx),
    )
}
