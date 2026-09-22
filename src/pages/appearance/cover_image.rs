//! An image that fills the box it is given and crops what will not fit.
//!
//! Masonry's own [`Image`](xilem::masonry::widgets::Image) widget with
//! [`ObjectFit::Cover`] *reports* the size the covering image wants — width
//! from the constraints, height from the aspect ratio — so a tile in a grid of
//! mixed shapes grows out of its cell and paints over its neighbours. This one
//! takes the cell as its size and clips the overflow inside a rounded rect,
//! which is the whole difference and the reason it exists.

use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Widget, WidgetMut,
};
use xilem::masonry::kurbo::{Affine, RoundedRect, Size};
use xilem::masonry::peniko::BlendMode;
use xilem::masonry::properties::ObjectFit;
use xilem::masonry::vello::Scene;
use xilem::{ImageBrush, Pod, ViewCtx};

// --- MARK: Widget ---

/// The Masonry-side widget backing [`cover_image`].
pub struct CoverImageWidget {
    brush: ImageBrush,
    corner_radius: f64,
}

impl CoverImageWidget {
    fn set_brush(this: &mut WidgetMut<'_, Self>, brush: ImageBrush) {
        this.widget.brush = brush;
        this.ctx.request_paint_only();
    }

    fn set_corner_radius(this: &mut WidgetMut<'_, Self>, corner_radius: f64) {
        this.widget.corner_radius = corner_radius;
        this.ctx.request_paint_only();
    }
}

impl Widget for CoverImageWidget {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    /// The box, whatever it is — a cover fills it by definition. Give it a
    /// bounded one: an unconstrained parent has no size to cover.
    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let source = Size::new(
            f64::from(self.brush.image.width),
            f64::from(self.brush.image.height),
        );
        let clip = RoundedRect::from_rect(size.to_rect(), self.corner_radius);
        scene.push_layer(BlendMode::default(), 1.0, Affine::IDENTITY, &clip);
        scene.draw_image(&self.brush, ObjectFit::Cover.affine_to_fill(size, source));
        scene.pop_layer();
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// --- MARK: View ---

/// An image scaled to cover its box, centered, cropped on the long axis.
pub fn cover_image(brush: ImageBrush) -> CoverImage {
    CoverImage {
        brush,
        corner_radius: 0.0,
    }
}

#[must_use = "View values do nothing unless returned from a view function"]
pub struct CoverImage {
    brush: ImageBrush,
    corner_radius: f64,
}

impl CoverImage {
    /// Rounds the crop's corners.
    pub fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = radius;
        self
    }
}

impl ViewMarker for CoverImage {}
impl<State, Action> View<State, Action, ViewCtx> for CoverImage
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<CoverImageWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(CoverImageWidget {
                brush: self.brush.clone(),
                corner_radius: self.corner_radius,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.brush != self.brush {
            CoverImageWidget::set_brush(&mut element, self.brush.clone());
        }
        if prev.corner_radius != self.corner_radius {
            CoverImageWidget::set_corner_radius(&mut element, self.corner_radius);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Nop
    }
}
