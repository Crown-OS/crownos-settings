//! Named transitions, and which one a list of effects amounts to.

use crownos_config::schema::{Edge, Effect};

use crate::controls::Options;

const BLUR_RADIUS: f32 = 24.0;
const SOFT_BLUR_RADIUS: f32 = 12.0;
const SLIDE_DISTANCE: f32 = 40.0;
const ZOOM_FROM: f32 = 0.94;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    None,
    Fade,
    Blur,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Zoom,
    BlurSlide,
    Custom,
}

pub static PRESETS: Options<Preset> = Options::new(&[
    ("None", Preset::None),
    ("Fade", Preset::Fade),
    ("Blur", Preset::Blur),
    ("Slide up", Preset::SlideUp),
    ("Slide down", Preset::SlideDown),
    ("Slide left", Preset::SlideLeft),
    ("Slide right", Preset::SlideRight),
    ("Zoom", Preset::Zoom),
    ("Blur + slide", Preset::BlurSlide),
    ("Custom", Preset::Custom),
]);

impl Preset {
    /// The effects it stands for; `None` for [`Custom`](Self::Custom), which
    /// is whatever was edited by hand.
    pub fn effects(self) -> Option<Vec<Effect>> {
        let faded = |effect| Some(vec![Effect::Fade, effect]);
        match self {
            Self::None => Some(Vec::new()),
            Self::Fade => Some(vec![Effect::Fade]),
            Self::Blur => faded(Effect::Blur {
                radius: BLUR_RADIUS,
            }),
            Self::SlideUp => faded(slide(Edge::Bottom)),
            Self::SlideDown => faded(slide(Edge::Top)),
            Self::SlideLeft => faded(slide(Edge::Right)),
            Self::SlideRight => faded(slide(Edge::Left)),
            Self::Zoom => faded(Effect::Zoom { from: ZOOM_FROM }),
            Self::BlurSlide => Some(vec![
                Effect::Fade,
                Effect::Blur {
                    radius: SOFT_BLUR_RADIUS,
                },
                slide(Edge::Bottom),
            ]),
            Self::Custom => None,
        }
    }

    /// The preset `effects` amount to, in whatever order they are listed.
    pub fn of(effects: &[Effect]) -> Self {
        PRESETS
            .values()
            .find(|preset| {
                preset
                    .effects()
                    .is_some_and(|named| same_effects(&named, effects))
            })
            .unwrap_or(Self::Custom)
    }
}

fn slide(from: Edge) -> Effect {
    Effect::Slide {
        from,
        distance: SLIDE_DISTANCE,
    }
}

fn same_effects(left: &[Effect], right: &[Effect]) -> bool {
    left.len() == right.len() && left.iter().all(|effect| right.contains(effect))
}

#[cfg(test)]
mod tests {
    use crownos_config::schema::LockTransitions;

    use super::*;

    #[test]
    fn every_preset_is_recognised_from_its_own_effects() {
        for preset in PRESETS.values() {
            match preset.effects() {
                Some(effects) => assert_eq!(Preset::of(&effects), preset),
                None => assert_eq!(preset, Preset::Custom),
            }
        }
    }

    #[test]
    fn the_stock_transitions_are_named_presets() {
        let stock = LockTransitions::default();
        assert_eq!(Preset::of(&stock.appear.effects), Preset::Blur);
        assert_eq!(Preset::of(&stock.reveal.effects), Preset::BlurSlide);
        assert_eq!(Preset::of(&stock.dismiss.effects), Preset::Zoom);
    }

    #[test]
    fn order_does_not_matter_but_any_edit_makes_it_custom() {
        let reordered = [Effect::Blur { radius: 24.0 }, Effect::Fade];
        assert_eq!(Preset::of(&reordered), Preset::Blur);
        let edited = [Effect::Fade, Effect::Blur { radius: 30.0 }];
        assert_eq!(Preset::of(&edited), Preset::Custom);
        assert_eq!(Preset::of(&[Effect::Fade, Effect::Fade]), Preset::Custom);
    }
}
