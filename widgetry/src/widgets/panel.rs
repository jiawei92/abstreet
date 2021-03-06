use crate::{
    AreaSlider, Autocomplete, Checkbox, Color, Dropdown, EventCtx, GfxCtx, HorizontalAlignment,
    Menu, Outcome, PersistentSplit, ScreenDims, ScreenPt, ScreenRectangle, Slider, Spinner,
    TextBox, VerticalAlignment, Widget, WidgetImpl, WidgetOutput,
};
use geom::{Percent, Polygon};
use std::collections::HashSet;
use stretch::geometry::Size;
use stretch::node::Stretch;
use stretch::number::Number;
use stretch::style::{Dimension, Style};

pub struct Panel {
    pub(crate) top_level: Widget,
    horiz: HorizontalAlignment,
    vert: VerticalAlignment,
    dims: Dims,

    scrollable_x: bool,
    scrollable_y: bool,
    contents_dims: ScreenDims,
    container_dims: ScreenDims,
    clip_rect: Option<ScreenRectangle>,
}

impl Panel {
    pub fn new(top_level: Widget) -> PanelBuilder {
        PanelBuilder {
            top_level,
            horiz: HorizontalAlignment::Center,
            vert: VerticalAlignment::Center,
            dims: Dims::MaxPercent(Percent::int(100), Percent::int(100)),
        }
    }

    fn recompute_layout(&mut self, ctx: &EventCtx, recompute_bg: bool) {
        let mut stretch = Stretch::new();
        let root = stretch
            .new_node(
                Style {
                    ..Default::default()
                },
                Vec::new(),
            )
            .unwrap();

        let mut nodes = vec![];
        self.top_level.get_flexbox(root, &mut stretch, &mut nodes);
        nodes.reverse();

        // TODO Express more simply. Constraining this seems useless.
        let container_size = Size {
            width: Number::Undefined,
            height: Number::Undefined,
        };
        stretch.compute_layout(root, container_size).unwrap();

        // TODO I'm so confused why these 2 are acting differently. :(
        let effective_dims = if self.scrollable_x || self.scrollable_y {
            self.container_dims
        } else {
            let result = stretch.layout(root).unwrap();
            ScreenDims::new(result.size.width.into(), result.size.height.into())
        };
        let top_left = ctx
            .canvas
            .align_window(effective_dims, self.horiz, self.vert);
        let offset = self.scroll_offset();
        self.top_level.apply_flexbox(
            &stretch,
            &mut nodes,
            top_left.x,
            top_left.y,
            offset,
            ctx,
            recompute_bg,
            false,
        );
        assert!(nodes.is_empty());
    }

    fn scroll_offset(&self) -> (f64, f64) {
        let x = if self.scrollable_x {
            self.slider("horiz scrollbar").get_percent()
                * (self.contents_dims.width - self.container_dims.width).max(0.0)
        } else {
            0.0
        };
        let y = if self.scrollable_y {
            self.slider("vert scrollbar").get_percent()
                * (self.contents_dims.height - self.container_dims.height).max(0.0)
        } else {
            0.0
        };
        (x, y)
    }

    fn set_scroll_offset(&mut self, ctx: &EventCtx, offset: (f64, f64)) {
        let mut changed = false;
        if self.scrollable_x {
            changed = true;
            let max = (self.contents_dims.width - self.container_dims.width).max(0.0);
            if max == 0.0 {
                self.slider_mut("horiz scrollbar").set_percent(ctx, 0.0);
            } else {
                self.slider_mut("horiz scrollbar")
                    .set_percent(ctx, abstutil::clamp(offset.0, 0.0, max) / max);
            }
        }
        if self.scrollable_y {
            changed = true;
            let max = (self.contents_dims.height - self.container_dims.height).max(0.0);
            if max == 0.0 {
                self.slider_mut("vert scrollbar").set_percent(ctx, 0.0);
            } else {
                self.slider_mut("vert scrollbar")
                    .set_percent(ctx, abstutil::clamp(offset.1, 0.0, max) / max);
            }
        }
        if changed {
            self.recompute_layout(ctx, false);
        }
    }

    pub fn event(&mut self, ctx: &mut EventCtx) -> Outcome {
        if (self.scrollable_x || self.scrollable_y)
            && ctx
                .canvas
                .get_cursor_in_screen_space()
                .map(|pt| self.top_level.rect.contains(pt))
                .unwrap_or(false)
        {
            if let Some((dx, dy)) = ctx.input.get_mouse_scroll() {
                let x_offset = if self.scrollable_x {
                    self.scroll_offset().0 + dx * (ctx.canvas.gui_scroll_speed as f64)
                } else {
                    0.0
                };
                let y_offset = if self.scrollable_y {
                    self.scroll_offset().1 - dy * (ctx.canvas.gui_scroll_speed as f64)
                } else {
                    0.0
                };
                self.set_scroll_offset(ctx, (x_offset, y_offset));
            }
        }

        if ctx.input.is_window_resized() {
            self.recompute_layout(ctx, false);
        }

        let before = self.scroll_offset();
        let mut output = WidgetOutput::new();
        self.top_level.widget.event(ctx, &mut output);
        if self.scroll_offset() != before || output.redo_layout {
            self.recompute_layout(ctx, true);
        }

        output.outcome
    }

    pub fn draw(&self, g: &mut GfxCtx) {
        if let Some(ref rect) = self.clip_rect {
            g.enable_clipping(rect.clone());
            g.canvas.mark_covered_area(rect.clone());
        } else {
            g.canvas.mark_covered_area(self.top_level.rect.clone());
        }

        // Debugging
        if false {
            g.fork_screenspace();
            g.draw_polygon(Color::RED.alpha(0.5), self.top_level.rect.to_polygon());

            let top_left = g
                .canvas
                .align_window(self.container_dims, self.horiz, self.vert);
            g.draw_polygon(
                Color::BLUE.alpha(0.5),
                Polygon::rectangle(self.container_dims.width, self.container_dims.height)
                    .translate(top_left.x, top_left.y),
            );
        }

        g.unfork();

        self.top_level.draw(g);
        if self.scrollable_x || self.scrollable_y {
            g.disable_clipping();

            // Draw the scrollbars after clipping is disabled, because they actually live just
            // outside the rectangle.
            if self.scrollable_x {
                self.slider("horiz scrollbar").draw(g);
            }
            if self.scrollable_y {
                self.slider("vert scrollbar").draw(g);
            }
        }
    }

    pub fn get_all_click_actions(&self) -> HashSet<String> {
        let mut actions = HashSet::new();
        self.top_level.get_all_click_actions(&mut actions);
        actions
    }

    pub fn restore(&mut self, ctx: &mut EventCtx, prev: &Panel) {
        self.set_scroll_offset(ctx, prev.scroll_offset());

        self.top_level.restore(ctx, &prev);

        // Since we just moved things around, let all widgets respond to the mouse being somewhere
        ctx.no_op_event(true, |ctx| assert_eq!(self.event(ctx), Outcome::Nothing));
    }

    pub fn scroll_to_member(&mut self, ctx: &EventCtx, name: String) {
        if let Some(w) = self.top_level.find(&name) {
            let y1 = w.rect.y1;
            self.set_scroll_offset(ctx, (0.0, y1));
        } else {
            panic!("Can't scroll_to_member of unknown {}", name);
        }
    }

    pub fn has_widget(&self, name: &str) -> bool {
        self.top_level.find(name).is_some()
    }

    pub fn slider(&self, name: &str) -> &Slider {
        self.find(name)
    }
    pub fn slider_mut(&mut self, name: &str) -> &mut Slider {
        self.find_mut(name)
    }
    pub fn area_slider(&self, name: &str) -> &AreaSlider {
        self.find(name)
    }

    pub fn take_menu_choice<T: 'static>(&mut self, name: &str) -> T {
        self.find_mut::<Menu<T>>(name).take_current_choice()
    }

    pub fn is_checked(&self, name: &str) -> bool {
        self.find::<Checkbox>(name).enabled
    }
    pub fn maybe_is_checked(&self, name: &str) -> Option<bool> {
        if self.has_widget(name) {
            Some(self.find::<Checkbox>(name).enabled)
        } else {
            None
        }
    }

    pub fn text_box(&self, name: &str) -> String {
        self.find::<TextBox>(name).get_line()
    }

    pub fn spinner(&self, name: &str) -> isize {
        self.find::<Spinner>(name).current
    }

    pub fn dropdown_value<T: 'static + PartialEq + Clone>(&self, name: &str) -> T {
        self.find::<Dropdown<T>>(name).current_value()
    }
    pub fn persistent_split_value<T: 'static + PartialEq + Clone>(&self, name: &str) -> T {
        self.find::<PersistentSplit<T>>(name).current_value()
    }

    pub fn autocomplete_done<T: 'static + Clone>(&self, name: &str) -> Option<Vec<T>> {
        self.find::<Autocomplete<T>>(name).final_value()
    }

    pub fn find<T: WidgetImpl>(&self, name: &str) -> &T {
        if let Some(w) = self.top_level.find(name) {
            if let Some(x) = w.widget.downcast_ref::<T>() {
                x
            } else {
                panic!("Found widget {}, but wrong type", name);
            }
        } else {
            panic!("Can't find widget {}", name);
        }
    }
    pub fn find_mut<T: WidgetImpl>(&mut self, name: &str) -> &mut T {
        if let Some(w) = self.top_level.find_mut(name) {
            if let Some(x) = w.widget.downcast_mut::<T>() {
                x
            } else {
                panic!("Found widget {}, but wrong type", name);
            }
        } else {
            panic!("Can't find widget {}", name);
        }
    }

    pub fn rect_of(&self, name: &str) -> &ScreenRectangle {
        &self.top_level.find(name).unwrap().rect
    }
    // TODO Deprecate
    pub fn center_of(&self, name: &str) -> ScreenPt {
        self.rect_of(name).center()
    }
    pub fn center_of_panel(&self) -> ScreenPt {
        self.top_level.rect.center()
    }

    pub fn align_above(&mut self, ctx: &mut EventCtx, other: &Panel) {
        // Small padding
        self.vert = VerticalAlignment::Above(other.top_level.rect.y1 - 5.0);
        self.recompute_layout(ctx, false);

        // Since we just moved things around, let all widgets respond to the mouse being somewhere
        ctx.no_op_event(true, |ctx| assert_eq!(self.event(ctx), Outcome::Nothing));
    }
    pub fn align_below(&mut self, ctx: &mut EventCtx, other: &Panel, pad: f64) {
        self.vert = VerticalAlignment::Below(other.top_level.rect.y2 + pad);
        self.recompute_layout(ctx, false);

        // Since we just moved things around, let all widgets respond to the mouse being somewhere
        ctx.no_op_event(true, |ctx| assert_eq!(self.event(ctx), Outcome::Nothing));
    }

    // All margins/padding/etc from the previous widget are retained.
    pub fn replace(&mut self, ctx: &mut EventCtx, id: &str, mut new: Widget) {
        let old = self.top_level.find_mut(id).unwrap();
        new.layout.style = old.layout.style;
        *old = new;
        self.recompute_layout(ctx, true);

        // TODO Same no_op_event as align_above? Should we always do this in recompute_layout?
    }

    pub fn clicked_outside(&self, ctx: &mut EventCtx) -> bool {
        // TODO No great way to populate OSD from here with "click to cancel"
        !self.top_level.rect.contains(ctx.canvas.get_cursor()) && ctx.normal_left_click()
    }

    pub fn currently_hovering(&self) -> Option<&String> {
        self.top_level.currently_hovering()
    }
}

pub struct PanelBuilder {
    top_level: Widget,
    horiz: HorizontalAlignment,
    vert: VerticalAlignment,
    dims: Dims,
}

enum Dims {
    MaxPercent(Percent, Percent),
    ExactPercent(f64, f64),
}

impl PanelBuilder {
    pub fn build(mut self, ctx: &mut EventCtx) -> Panel {
        self.top_level = self.top_level.padding(16).bg(ctx.style.panel_bg);
        self.build_custom(ctx)
    }

    pub fn build_custom(self, ctx: &mut EventCtx) -> Panel {
        let mut c = Panel {
            top_level: self.top_level,

            horiz: self.horiz,
            vert: self.vert,
            dims: self.dims,

            scrollable_x: false,
            scrollable_y: false,
            contents_dims: ScreenDims::new(0.0, 0.0),
            container_dims: ScreenDims::new(0.0, 0.0),
            clip_rect: None,
        };
        if let Dims::ExactPercent(w, h) = c.dims {
            // Don't set size, because then scrolling breaks -- the actual size has to be based on
            // the contents.
            c.top_level.layout.style.min_size = Size {
                width: Dimension::Points((w * ctx.canvas.window_width) as f32),
                height: Dimension::Points((h * ctx.canvas.window_height) as f32),
            };
        }
        c.recompute_layout(ctx, false);

        c.contents_dims = ScreenDims::new(c.top_level.rect.width(), c.top_level.rect.height());
        c.container_dims = match c.dims {
            Dims::MaxPercent(w, h) => ScreenDims::new(
                c.contents_dims
                    .width
                    .min(w.inner() * ctx.canvas.window_width),
                c.contents_dims
                    .height
                    .min(h.inner() * ctx.canvas.window_height),
            ),
            Dims::ExactPercent(w, h) => {
                ScreenDims::new(w * ctx.canvas.window_width, h * ctx.canvas.window_height)
            }
        };

        // If the panel fits without a scrollbar, don't add one.
        let top_left = ctx.canvas.align_window(c.container_dims, c.horiz, c.vert);
        if c.contents_dims.width > c.container_dims.width {
            c.scrollable_x = true;
            c.top_level = Widget::custom_col(vec![
                c.top_level,
                Slider::horizontal(
                    ctx,
                    c.container_dims.width,
                    c.container_dims.width * (c.container_dims.width / c.contents_dims.width),
                    0.0,
                )
                .named("horiz scrollbar")
                .abs(top_left.x, top_left.y + c.container_dims.height),
            ]);
        }
        if c.contents_dims.height > c.container_dims.height {
            c.scrollable_y = true;
            c.top_level = Widget::custom_row(vec![
                c.top_level,
                Slider::vertical(
                    ctx,
                    c.container_dims.height,
                    c.container_dims.height * (c.container_dims.height / c.contents_dims.height),
                    0.0,
                )
                .named("vert scrollbar")
                .abs(top_left.x + c.container_dims.width, top_left.y),
            ]);
        }
        if c.scrollable_x || c.scrollable_y {
            c.recompute_layout(ctx, false);
            c.clip_rect = Some(ScreenRectangle::top_left(top_left, c.container_dims));
        }

        // Just trigger error if a button is double-defined
        c.get_all_click_actions();
        // Let all widgets initially respond to the mouse being somewhere
        ctx.no_op_event(true, |ctx| assert_eq!(c.event(ctx), Outcome::Nothing));
        c
    }

    pub fn aligned(mut self, horiz: HorizontalAlignment, vert: VerticalAlignment) -> PanelBuilder {
        self.horiz = horiz;
        self.vert = vert;
        self
    }

    pub fn max_size(mut self, width: Percent, height: Percent) -> PanelBuilder {
        if width == Percent::int(100) && height == Percent::int(100) {
            panic!("By default, Panels are capped at 100% of the screen. This is redundant.");
        }
        self.dims = Dims::MaxPercent(width, height);
        self
    }

    pub fn exact_size_percent(mut self, pct_width: usize, pct_height: usize) -> PanelBuilder {
        self.dims = Dims::ExactPercent((pct_width as f64) / 100.0, (pct_height as f64) / 100.0);
        self
    }
}
