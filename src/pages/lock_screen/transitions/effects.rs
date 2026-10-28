//! What can be edited about each kind of [`Effect`]: one table entry per
//! variant, so a new variant is one more entry and the cards follow.

use std::mem::discriminant;

use crownos_config::schema::{Edge, Effect};

use crate::controls::Options;

pub struct EffectKind {
    pub name: &'static str,
    pub description: &'static str,
    /// What switching the effect on adds, and the variant it stands for.
    pub default: Effect,
    pub params: &'static [Param],
}

pub enum Param {
    /// A number, shown as the stored value times `shown_per_stored`.
    Amount {
        label: &'static str,
        shown: (f32, f32),
        step: f32,
        unit: &'static str,
        shown_per_stored: f32,
        read: fn(&Effect) -> Option<f32>,
        write: fn(&mut Effect, f32),
    },
    Edge {
        label: &'static str,
        read: fn(&Effect) -> Option<Edge>,
        write: fn(&mut Effect, Edge),
    },
}

pub static EDGES: Options<Edge> = Options::new(&[
    ("Below", Edge::Bottom),
    ("Above", Edge::Top),
    ("The left", Edge::Left),
    ("The right", Edge::Right),
]);

pub static EFFECTS: &[EffectKind] = &[
    EffectKind {
        name: "Fade",
        description: "In from transparent",
        default: Effect::Fade,
        params: &[],
    },
    EffectKind {
        name: "Blur",
        description: "In from out of focus",
        default: Effect::Blur { radius: 24.0 },
        params: &[Param::Amount {
            label: "Blur radius",
            shown: (0.0, 64.0),
            step: 1.0,
            unit: "px",
            shown_per_stored: 1.0,
            read: |effect| match effect {
                Effect::Blur { radius } => Some(*radius),
                _ => None,
            },
            write: |effect, value| {
                if let Effect::Blur { radius } = effect {
                    *radius = value;
                }
            },
        }],
    },
    EffectKind {
        name: "Slide",
        description: "In from one side",
        default: Effect::Slide {
            from: Edge::Bottom,
            distance: 40.0,
        },
        params: &[
            Param::Edge {
                label: "From",
                read: |effect| match effect {
                    Effect::Slide { from, .. } => Some(*from),
                    _ => None,
                },
                write: |effect, edge| {
                    if let Effect::Slide { from, .. } = effect {
                        *from = edge;
                    }
                },
            },
            Param::Amount {
                label: "Distance",
                shown: (0.0, 600.0),
                step: 1.0,
                unit: "px",
                shown_per_stored: 1.0,
                read: |effect| match effect {
                    Effect::Slide { distance, .. } => Some(*distance),
                    _ => None,
                },
                write: |effect, value| {
                    if let Effect::Slide { distance, .. } = effect {
                        *distance = value;
                    }
                },
            },
        ],
    },
    EffectKind {
        name: "Zoom",
        description: "In from smaller or larger",
        default: Effect::Zoom { from: 0.94 },
        params: &[Param::Amount {
            label: "Starting size",
            shown: (50.0, 150.0),
            step: 1.0,
            unit: "%",
            shown_per_stored: 100.0,
            read: |effect| match effect {
                Effect::Zoom { from } => Some(*from),
                _ => None,
            },
            write: |effect, value| {
                if let Effect::Zoom { from } = effect {
                    *from = value;
                }
            },
        }],
    },
];

impl EffectKind {
    pub fn is_kind_of(&self, effect: &Effect) -> bool {
        discriminant(effect) == discriminant(&self.default)
    }

    /// Adds this effect to `effects` or takes it out, keeping table order.
    pub fn switch(&self, effects: &mut Vec<Effect>, on: bool) {
        effects.retain(|effect| !self.is_kind_of(effect));
        if on {
            effects.push(self.default);
            effects.sort_by_key(rank);
        }
    }
}

fn rank(effect: &Effect) -> usize {
    EFFECTS
        .iter()
        .position(|kind| kind.is_kind_of(effect))
        .unwrap_or(EFFECTS.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_exactly_one_entry() {
        let variants = [
            Effect::Fade,
            Effect::Blur { radius: 1.0 },
            Effect::Slide {
                from: Edge::Top,
                distance: 1.0,
            },
            Effect::Zoom { from: 1.0 },
        ];
        for effect in &variants {
            let kinds = EFFECTS.iter().filter(|kind| kind.is_kind_of(effect));
            assert_eq!(kinds.count(), 1, "{effect:?}");
        }
    }

    #[test]
    fn switching_keeps_table_order_and_edits_only_its_own_kind() {
        let [fade, blur, slide, _] = [0, 1, 2, 3].map(|index| &EFFECTS[index]);
        let mut effects = vec![Effect::Slide {
            from: Edge::Left,
            distance: 9.0,
        }];
        blur.switch(&mut effects, true);
        fade.switch(&mut effects, true);
        assert!(fade.is_kind_of(&effects[0]) && blur.is_kind_of(&effects[1]));
        assert_eq!(
            effects[2],
            Effect::Slide {
                from: Edge::Left,
                distance: 9.0
            }
        );
        slide.switch(&mut effects, false);
        assert_eq!(effects.len(), 2);
    }

    #[test]
    fn parameters_only_touch_their_own_variant() {
        let Param::Amount { read, write, .. } = &EFFECTS[1].params[0] else {
            panic!("blur has a radius");
        };
        let mut fade = Effect::Fade;
        write(&mut fade, 9.0);
        assert_eq!(fade, Effect::Fade);
        assert_eq!(read(&Effect::Blur { radius: 9.0 }), Some(9.0));
        assert_eq!(read(&fade), None);
    }
}
