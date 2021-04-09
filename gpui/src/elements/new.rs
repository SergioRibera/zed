use crate::{
    geometry::{rect::RectF, vector::Vector2F},
    json, AfterLayoutContext, DebugContext, Event, EventContext, LayoutContext, PaintContext,
    SizeConstraint,
};
use core::panic;
use replace_with::replace_with_or_abort;
use std::{any::Any, borrow::Cow};

trait AnyElement {
    fn layout(&mut self, constraint: SizeConstraint, ctx: &mut LayoutContext) -> Vector2F;
    fn after_layout(&mut self, _: &mut AfterLayoutContext) {}
    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext);
    fn dispatch_event(&mut self, event: &Event, ctx: &mut EventContext) -> bool;
    fn debug(&self, ctx: &DebugContext) -> serde_json::Value;

    fn size(&self) -> Vector2F;
    fn metadata(&self) -> Option<&dyn Any>;
}

pub trait Element {
    type LayoutState;
    type PaintState;

    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
    ) -> (Vector2F, Self::LayoutState);

    fn after_layout(
        &mut self,
        size: Vector2F,
        layout: &mut Self::LayoutState,
        ctx: &mut AfterLayoutContext,
    );

    fn paint(
        &mut self,
        bounds: RectF,
        layout: &mut Self::LayoutState,
        ctx: &mut PaintContext,
    ) -> Self::PaintState;

    fn dispatch_event(
        &mut self,
        event: &Event,
        bounds: RectF,
        layout: &mut Self::LayoutState,
        paint: &mut Self::PaintState,
        ctx: &mut EventContext,
    ) -> bool;

    fn metadata(&self) -> Option<&dyn Any> {
        None
    }

    fn debug(
        &self,
        bounds: RectF,
        layout: &Self::LayoutState,
        paint: &Self::PaintState,
        ctx: &DebugContext,
    ) -> serde_json::Value;

    fn boxed(self) -> ElementBox
    where
        Self: 'static + Sized,
    {
        ElementBox {
            name: None,
            element: Box::new(Lifecycle::Init { element: self }),
        }
    }

    fn named(self, name: impl Into<Cow<'static, str>>) -> ElementBox
    where
        Self: 'static + Sized,
    {
        ElementBox {
            name: Some(name.into()),
            element: Box::new(Lifecycle::Init { element: self }),
        }
    }
}

pub enum Lifecycle<T: Element> {
    Init {
        element: T,
    },
    PostLayout {
        element: T,
        size: Vector2F,
        layout: T::LayoutState,
    },
    PostPaint {
        element: T,
        bounds: RectF,
        layout: T::LayoutState,
        paint: T::PaintState,
    },
}
pub struct ElementBox {
    name: Option<Cow<'static, str>>,
    element: Box<dyn AnyElement>,
}

impl<T: Element> AnyElement for Lifecycle<T> {
    fn layout(&mut self, constraint: SizeConstraint, ctx: &mut LayoutContext) -> Vector2F {
        let mut result = None;
        replace_with_or_abort(self, |me| match me {
            Lifecycle::Init { mut element }
            | Lifecycle::PostLayout { mut element, .. }
            | Lifecycle::PostPaint { mut element, .. } => {
                let (size, layout) = element.layout(constraint, ctx);
                debug_assert!(size.x().is_finite());
                debug_assert!(size.y().is_finite());

                result = Some(size);
                Lifecycle::PostLayout {
                    element,
                    size,
                    layout,
                }
            }
        });
        result.unwrap()
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext) {
        if let Lifecycle::PostLayout {
            element,
            size,
            layout,
        } = self
        {
            element.after_layout(*size, layout, ctx);
        } else {
            panic!("invalid element lifecycle state");
        }
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext) {
        replace_with_or_abort(self, |me| {
            if let Lifecycle::PostLayout {
                mut element,
                size,
                mut layout,
            } = me
            {
                let bounds = RectF::new(origin, size);
                let paint = element.paint(bounds, &mut layout, ctx);
                Lifecycle::PostPaint {
                    element,
                    bounds,
                    layout,
                    paint,
                }
            } else {
                panic!("invalid element lifecycle state");
            }
        });
    }

    fn dispatch_event(&mut self, event: &Event, ctx: &mut EventContext) -> bool {
        if let Lifecycle::PostPaint {
            element,
            bounds,
            layout,
            paint,
        } = self
        {
            element.dispatch_event(event, *bounds, layout, paint, ctx)
        } else {
            panic!("invalid element lifecycle state");
        }
    }

    fn size(&self) -> Vector2F {
        match self {
            Lifecycle::Init { .. } => panic!("invalid element lifecycle state"),
            Lifecycle::PostLayout { size, .. } => *size,
            Lifecycle::PostPaint { bounds, .. } => bounds.size(),
        }
    }

    fn metadata(&self) -> Option<&dyn Any> {
        match self {
            Lifecycle::Init { element }
            | Lifecycle::PostLayout { element, .. }
            | Lifecycle::PostPaint { element, .. } => element.metadata(),
        }
    }

    fn debug(&self, ctx: &DebugContext) -> serde_json::Value {
        match self {
            Lifecycle::PostPaint {
                element,
                bounds,
                layout,
                paint,
            } => element.debug(*bounds, layout, paint, ctx),
            _ => panic!("invalid element lifecycle state"),
        }
    }
}

impl ElementBox {
    pub fn layout(&mut self, constraint: SizeConstraint, ctx: &mut LayoutContext) -> Vector2F {
        self.element.layout(constraint, ctx)
    }

    pub fn after_layout(&mut self, ctx: &mut AfterLayoutContext) {
        self.element.after_layout(ctx);
    }

    pub fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext) {
        self.element.paint(origin, ctx);
    }

    pub fn dispatch_event(&mut self, event: &Event, ctx: &mut EventContext) -> bool {
        self.element.dispatch_event(event, ctx)
    }

    pub fn size(&self) -> Vector2F {
        self.element.size()
    }

    pub fn metadata(&self) -> Option<&dyn Any> {
        self.element.metadata()
    }

    pub fn debug(&self, ctx: &DebugContext) -> json::Value {
        let mut value = self.element.debug(ctx);

        if let Some(name) = &self.name {
            if let json::Value::Object(map) = &mut value {
                let mut new_map: crate::json::Map<String, serde_json::Value> = Default::default();
                new_map.insert("name".into(), json::Value::String(name.to_string()));
                new_map.append(map);
                return json::Value::Object(new_map);
            }
        }

        value
    }
}
