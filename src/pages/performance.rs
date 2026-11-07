//! The Performance page: how much memory zram compresses into, and how big
//! the swap partition behind it is.

use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, icons,
    spinner, status,
};
use crownui::prelude::{
    AnyView, Cx, Element, Listeners, Runtime, Semantic, Signal, Styled, View, bind, spacer, vstack,
};

use crate::controls::stepped_slider;
use crate::layout::{
    body_text, nothing, optional, setting_row_content, setting_row_desc, settings_card,
    settings_card_titled, settings_divider, spread, when,
};
use crate::net::memory::{
    self, Memory, Resize, SWAP_GIB_MIN, SwapPartition, ZRAM_PERCENT_MAX, ZRAM_PERCENT_MIN,
    ZRAM_PERCENT_STEP,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

const ROW_SPINNER: f32 = 14.0;
const NOT_INSTALLED: &str = "Set up when CrownOS is installed";

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Performance",
    slug: "performance",
    icon: icons::GAUGE,
    keywords: &["memory", "ram", "zram", "swap", "compressed"],
    build,
};

/// What applying a change updates.
#[derive(Clone, Copy)]
struct Applier {
    memory: Signal<Option<Memory>>,
    busy: Signal<bool>,
    failure: Signal<Option<String>>,
}

impl Applier {
    fn apply(self, cx: &mut Cx, change: Resize) {
        self.busy.set(cx, true);
        self.failure.set(cx, None);
        let applied = async move {
            let failure = memory::resize(change)
                .await
                .err()
                .map(|error| format!("{error:#}"));
            (Memory::read().await, failure)
        };
        cx.spawn(applied, move |cx, (now, failure)| {
            self.busy.set(cx, false);
            self.failure.set(cx, failure);
            self.memory.set(cx, Some(now));
        });
    }

    fn is_busy(self, runtime: &mut Runtime) -> bool {
        self.busy.get(runtime)
    }
}

fn build(cx: &mut Cx, _store: Store) -> AnyView {
    let applier = Applier {
        memory: cx.signal(None),
        busy: cx.signal(false),
        failure: cx.signal(None),
    };
    cx.spawn(Memory::read(), move |cx, now| {
        applier.memory.set(cx, Some(now));
    });

    page(
        &PAGE,
        (
            optional(
                move |runtime| applier.memory.get(runtime),
                move |cx, now| memory_card(cx, now, applier),
            ),
            optional(
                move |runtime| applier.failure.get(runtime),
                move |_, message| failure_card(applier, message),
            ),
        ),
    )
}

fn memory_card(cx: &mut Cx, now: Memory, applier: Applier) -> impl View + use<> {
    let zram = match now.zram_percent {
        Some(percent) => zram_rows(cx, now, percent, applier).boxed(),
        None => unavailable("Compressed memory", NOT_INSTALLED).boxed(),
    };
    let swap = match now.swap {
        Some(partition) => swap_rows(cx, partition, applier).boxed(),
        None if now.zram_percent.is_none() => unavailable("Swap partition", NOT_INSTALLED).boxed(),
        None => unavailable(
            "Swap partition",
            "Only a swap partition right behind the system partition can be resized",
        )
        .boxed(),
    };
    settings_card_titled("Memory", (zram, settings_divider(), swap))
}

fn zram_rows(cx: &mut Cx, now: Memory, applied: u32, applier: Applier) -> impl View + use<> {
    let percent = cx.signal(applied as f32);
    let chosen = move |runtime: &mut Runtime| percent.get(runtime).round() as u32;
    vstack((
        setting_row_desc(
            "Compressed memory",
            bind(move |runtime| {
                let chosen = chosen(runtime);
                format!("{chosen}% of RAM · up to {:.1} GiB", now.zram_gib(chosen))
            }),
            stepped_slider(
                percent,
                ZRAM_PERCENT_MIN as f32,
                ZRAM_PERCENT_MAX as f32,
                ZRAM_PERCENT_STEP as f32,
            )
            .disabled(bind(move |runtime| applier.is_busy(runtime))),
        ),
        apply_row(
            move |runtime| {
                (chosen(runtime) != applied).then(|| Resize::Zram {
                    percent: chosen(runtime),
                })
            },
            "Memory in use moves out while it resizes",
            applier,
        ),
    ))
}

fn swap_rows(cx: &mut Cx, partition: SwapPartition, applier: Applier) -> impl View + use<> {
    let size = cx.signal(partition.gib as f32);
    let chosen = move |runtime: &mut Runtime| size.get(runtime).round() as u32;
    vstack((
        setting_row_desc(
            "Swap partition",
            bind(move |runtime| {
                format!(
                    "{} GiB on disk · used once compressed memory fills",
                    chosen(runtime)
                )
            }),
            stepped_slider(size, SWAP_GIB_MIN as f32, partition.max_gib as f32, 1.0)
                .disabled(bind(move |runtime| applier.is_busy(runtime))),
        ),
        apply_row(
            move |runtime| {
                (chosen(runtime) != partition.gib).then(|| Resize::Swap {
                    gib: chosen(runtime),
                })
            },
            "The system partition shrinks or grows to make room",
            applier,
        ),
    ))
}

/// Shown while the slider is away from what the system uses.
fn apply_row(
    pending: impl FnMut(&mut Runtime) -> Option<Resize> + Copy + 'static,
    consequence: &'static str,
    applier: Applier,
) -> impl View {
    let mut changed = pending;
    when(
        move |runtime| changed(runtime).is_some(),
        move |_| {
            let mut pending = pending;
            setting_row_content(spread((
                body_text(consequence),
                spacer(),
                when(
                    move |runtime| applier.is_busy(runtime),
                    |_| spinner().size(ROW_SPINNER),
                ),
                row_button("Apply", ButtonVariant::Primary)
                    .disabled(bind(move |runtime| applier.is_busy(runtime)))
                    .on_click(move |cx, _| {
                        if let Some(change) = cx.untrack(|runtime| pending(runtime)) {
                            applier.apply(cx, change);
                        }
                    }),
            )))
        },
    )
}

fn row_button(label: &'static str, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

fn unavailable(title: &'static str, reason: &'static str) -> impl View {
    setting_row_desc(title, reason, nothing())
}

fn failure_card(applier: Applier, message: String) -> impl View {
    settings_card((setting_row_content(spread((
        status(message).tone(Tone::Danger),
        spacer(),
        row_button("Dismiss", ButtonVariant::Ghost)
            .on_click(move |cx, _| applier.failure.set(cx, None)),
    ))),))
}
