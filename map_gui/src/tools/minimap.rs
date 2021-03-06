use abstutil::clamp;
use geom::{Distance, Polygon, Pt2D, Ring};
use widgetry::{
    Btn, Color, Drawable, EventCtx, Filler, GeomBatch, GfxCtx, HorizontalAlignment, Line, Outcome,
    Panel, ScreenPt, Spinner, Transition, VerticalAlignment, Widget,
};

use crate::tools::Navigator;
use crate::SimpleApp;

// TODO Some of the math in here might assume map bound minimums start at (0, 0).
pub struct SimpleMinimap {
    dragging: bool,
    pub(crate) panel: Panel,
    // Update panel when other things change
    zoomed: bool,
    with_zorder: bool,

    // [0, 3], with 0 meaning the most unzoomed
    zoom_lvl: usize,
    base_zoom: f64,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
}

impl SimpleMinimap {
    pub fn new(ctx: &mut EventCtx, app: &SimpleApp, with_zorder: bool) -> SimpleMinimap {
        // Initially pick a zoom to fit the smaller of the entire map's width or height in the
        // minimap. Arbitrary and probably pretty weird.
        let bounds = app.map.get_bounds();
        let base_zoom = 0.15 * ctx.canvas.window_width / bounds.width().min(bounds.height());
        let mut m = SimpleMinimap {
            dragging: false,
            panel: Panel::empty(ctx),
            zoomed: ctx.canvas.cam_zoom >= app.opts.min_zoom_for_detail,
            with_zorder,

            zoom_lvl: 0,
            base_zoom,
            zoom: base_zoom,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        m.recreate_panel(ctx, app);
        if m.zoomed {
            m.recenter(ctx, app);
        }
        m
    }

    pub fn recreate_panel(&mut self, ctx: &mut EventCtx, app: &SimpleApp) {
        if ctx.canvas.cam_zoom < app.opts.min_zoom_for_detail {
            self.panel = Panel::empty(ctx);
            return;
        }

        let zoom_col = {
            let mut col = vec![Btn::svg_def("system/assets/speed/speed_up.svg")
                .build(ctx, "zoom in", None)
                .margin_below(20)];
            for i in (0..=3).rev() {
                let color = if self.zoom_lvl < i {
                    Color::WHITE.alpha(0.2)
                } else {
                    Color::WHITE
                };
                let rect = Polygon::rectangle(20.0, 8.0);
                col.push(
                    Btn::custom(
                        GeomBatch::from(vec![(color, rect.clone())]),
                        GeomBatch::from(vec![(app.cs.hovering, rect.clone())]),
                        rect,
                        None,
                    )
                    .build(ctx, format!("zoom to level {}", i + 1), None)
                    .margin_below(20),
                );
            }
            col.push(
                Btn::svg_def("system/assets/speed/slow_down.svg").build(ctx, "zoom out", None),
            );
            // The zoom column should start below the "pan up" arrow. But if we put it on the row
            // with <, minimap, and > then it messes up the horizontal alignment of the
            // pan up arrow. Also, double column to avoid the background color
            // stretching to the bottom of the row.
            Widget::custom_col(vec![
                Widget::custom_col(col).padding(10).bg(app.cs.inner_panel),
                if self.with_zorder {
                    Widget::col(vec![
                        Line("Z-order:").small().draw(ctx),
                        Spinner::new(ctx, app.draw_map.zorder_range, app.draw_map.show_zorder)
                            .named("zorder"),
                    ])
                    .margin_above(10)
                } else {
                    Widget::nothing()
                },
            ])
            .margin_above(26)
        };

        let minimap_controls = Widget::col(vec![
            Btn::svg_def("system/assets/minimap/up.svg")
                .build(ctx, "pan up", None)
                .centered_horiz(),
            Widget::row(vec![
                Btn::svg_def("system/assets/minimap/left.svg")
                    .build(ctx, "pan left", None)
                    .centered_vert(),
                Filler::square_width(ctx, 0.15).named("minimap"),
                Btn::svg_def("system/assets/minimap/right.svg")
                    .build(ctx, "pan right", None)
                    .centered_vert(),
            ]),
            Btn::svg_def("system/assets/minimap/down.svg")
                .build(ctx, "pan down", None)
                .centered_horiz(),
        ]);

        self.panel = Panel::new(Widget::row(vec![Widget::col(vec![
            // TODO Remove
            Widget::row(vec![minimap_controls, zoom_col]),
        ])
        .padding(16)
        .bg(app.cs.panel_bg)]))
        .aligned(
            HorizontalAlignment::Right,
            VerticalAlignment::BottomAboveOSD,
        )
        .build_custom(ctx);
    }

    fn map_to_minimap_pct(&self, pt: Pt2D) -> (f64, f64) {
        let inner_rect = self.panel.rect_of("minimap");
        let pct_x = (pt.x() * self.zoom - self.offset_x) / inner_rect.width();
        let pct_y = (pt.y() * self.zoom - self.offset_y) / inner_rect.height();
        (pct_x, pct_y)
    }

    pub fn set_zoom(&mut self, ctx: &mut EventCtx, app: &SimpleApp, zoom_lvl: usize) {
        // Make the frame wind up in the same relative position on the minimap
        let (pct_x, pct_y) = self.map_to_minimap_pct(ctx.canvas.center_to_map_pt());

        let zoom_speed: f64 = 2.0;
        self.zoom_lvl = zoom_lvl;
        self.zoom = self.base_zoom * zoom_speed.powi(self.zoom_lvl as i32);
        self.recreate_panel(ctx, app);

        // Find the new offset
        let map_center = ctx.canvas.center_to_map_pt();
        let inner_rect = self.panel.rect_of("minimap");
        self.offset_x = map_center.x() * self.zoom - pct_x * inner_rect.width();
        self.offset_y = map_center.y() * self.zoom - pct_y * inner_rect.height();
    }

    fn recenter(&mut self, ctx: &EventCtx, app: &SimpleApp) {
        // Recenter the minimap on the screen bounds
        let map_center = ctx.canvas.center_to_map_pt();
        let rect = self.panel.rect_of("minimap");
        let off_x = map_center.x() * self.zoom - rect.width() / 2.0;
        let off_y = map_center.y() * self.zoom - rect.height() / 2.0;

        // Don't go out of bounds.
        let bounds = app.map.get_bounds();
        // TODO For boundaries without rectangular shapes, it'd be even nicer to clamp to the
        // boundary.
        self.offset_x = off_x.max(0.0).min(bounds.max_x * self.zoom - rect.width());
        self.offset_y = off_y.max(0.0).min(bounds.max_y * self.zoom - rect.height());
    }

    pub fn event(
        &mut self,
        ctx: &mut EventCtx,
        app: &mut SimpleApp,
    ) -> Option<Transition<SimpleApp>> {
        let zoomed = ctx.canvas.cam_zoom >= app.opts.min_zoom_for_detail;
        if zoomed != self.zoomed {
            let just_zoomed_in = zoomed && !self.zoomed;

            self.zoomed = zoomed;
            self.recreate_panel(ctx, app);

            if just_zoomed_in {
                self.recenter(ctx, app);
            }
        } else if self.zoomed && !self.dragging {
            // If either corner of the cursor is out of bounds on the minimap, recenter.
            // TODO This means clicking the pan buttons while along the boundary won't work.
            let mut ok = true;
            for pt in vec![
                ScreenPt::new(0.0, 0.0),
                ScreenPt::new(ctx.canvas.window_width, ctx.canvas.window_height),
            ] {
                let (pct_x, pct_y) = self.map_to_minimap_pct(ctx.canvas.screen_to_map(pt));
                if pct_x < 0.0 || pct_x > 1.0 || pct_y < 0.0 || pct_y > 1.0 {
                    ok = false;
                    break;
                }
            }
            if !ok {
                self.recenter(ctx, app);
            }
        }
        if ctx.input.is_window_resized() {
            // When the window is resized, just reset completely. This is important when the window
            // size at startup is incorrect and immediately corrected by the window manager after
            // Minimap::new happens.
            let bounds = app.map.get_bounds();
            // On Windows, apparently minimizing can cause some resize events with 0, 0 dimensions!
            self.base_zoom =
                (0.15 * ctx.canvas.window_width / bounds.width().min(bounds.height())).max(0.0001);
            self.zoom = self.base_zoom;
            if self.zoomed {
                self.recenter(ctx, app);
            }
        }

        let pan_speed = 100.0;
        match self.panel.event(ctx) {
            Outcome::Clicked(x) => match x {
                x if x == "pan up" => {
                    self.offset_y -= pan_speed * self.zoom;
                    return Some(Transition::KeepWithMouseover);
                }
                x if x == "pan down" => {
                    self.offset_y += pan_speed * self.zoom;
                    return Some(Transition::KeepWithMouseover);
                }
                x if x == "pan left" => {
                    self.offset_x -= pan_speed * self.zoom;
                    return Some(Transition::KeepWithMouseover);
                }
                x if x == "pan right" => {
                    self.offset_x += pan_speed * self.zoom;
                    return Some(Transition::KeepWithMouseover);
                }
                // TODO Make the center of the cursor still point to the same thing. Same math as
                // Canvas.
                x if x == "zoom in" => {
                    if self.zoom_lvl != 3 {
                        self.set_zoom(ctx, app, self.zoom_lvl + 1);
                    }
                }
                x if x == "zoom out" => {
                    if self.zoom_lvl != 0 {
                        self.set_zoom(ctx, app, self.zoom_lvl - 1);
                    }
                }
                x if x == "zoom to level 1" => {
                    self.set_zoom(ctx, app, 0);
                }
                x if x == "zoom to level 2" => {
                    self.set_zoom(ctx, app, 1);
                }
                x if x == "zoom to level 3" => {
                    self.set_zoom(ctx, app, 2);
                }
                x if x == "zoom to level 4" => {
                    self.set_zoom(ctx, app, 3);
                }
                x if x == "search" => {
                    return Some(Transition::Push(Navigator::new(ctx, app)));
                }
                _ => unreachable!(),
            },
            Outcome::Changed => {
                app.draw_map.show_zorder = self.panel.spinner("zorder");
                self.recreate_panel(ctx, app);
            }
            _ => {}
        }

        if self.zoomed {
            let inner_rect = self.panel.rect_of("minimap");

            // TODO Not happy about reaching in like this. The minimap logic should be an widgetry
            // Widget eventually, a generalization of Canvas.
            let mut pt = ctx.canvas.get_cursor();
            if self.dragging {
                if ctx.input.left_mouse_button_released() {
                    self.dragging = false;
                }
                // Don't drag out of inner_rect
                pt.x = clamp(pt.x, inner_rect.x1, inner_rect.x2);
                pt.y = clamp(pt.y, inner_rect.y1, inner_rect.y2);
            } else if inner_rect.contains(pt) && ctx.input.left_mouse_button_pressed() {
                self.dragging = true;
            } else {
                return None;
            }

            let percent_x = (pt.x - inner_rect.x1) / inner_rect.width();
            let percent_y = (pt.y - inner_rect.y1) / inner_rect.height();

            let map_pt = Pt2D::new(
                (self.offset_x + percent_x * inner_rect.width()) / self.zoom,
                (self.offset_y + percent_y * inner_rect.height()) / self.zoom,
            );
            ctx.canvas.center_on_map_pt(map_pt);
        }

        None
    }

    pub fn draw(&self, g: &mut GfxCtx, app: &SimpleApp) {
        self.draw_with_extra_layers(g, app, Vec::new());
    }

    pub fn draw_with_extra_layers(&self, g: &mut GfxCtx, app: &SimpleApp, extra: Vec<&Drawable>) {
        self.panel.draw(g);
        if !self.zoomed {
            return;
        }

        let inner_rect = self.panel.rect_of("minimap").clone();

        let mut map_bounds = app.map.get_bounds().clone();
        // Adjust bounds to account for the current pan and zoom
        map_bounds.min_x = (map_bounds.min_x + self.offset_x) / self.zoom;
        map_bounds.min_y = (map_bounds.min_y + self.offset_y) / self.zoom;
        map_bounds.max_x = map_bounds.min_x + inner_rect.width() / self.zoom;
        map_bounds.max_y = map_bounds.min_y + inner_rect.height() / self.zoom;

        g.fork(
            Pt2D::new(map_bounds.min_x, map_bounds.min_y),
            ScreenPt::new(inner_rect.x1, inner_rect.y1),
            self.zoom,
            None,
        );
        g.enable_clipping(inner_rect);
        g.redraw(&app.draw_map.boundary_polygon);
        g.redraw(&app.draw_map.draw_all_areas);
        g.redraw(&app.draw_map.draw_all_unzoomed_parking_lots);
        g.redraw(&app.draw_map.draw_all_unzoomed_roads_and_intersections);
        g.redraw(&app.draw_map.draw_all_buildings);
        for draw in extra {
            g.redraw(draw);
        }
        // Not the building or parking lot paths

        // The cursor
        let (x1, y1) = {
            let pt = g.canvas.screen_to_map(ScreenPt::new(0.0, 0.0));
            (pt.x(), pt.y())
        };
        let (x2, y2) = {
            let pt = g
                .canvas
                .screen_to_map(ScreenPt::new(g.canvas.window_width, g.canvas.window_height));
            (pt.x(), pt.y())
        };
        g.draw_polygon(
            app.cs.minimap_cursor,
            Ring::must_new(vec![
                Pt2D::new(x1, y1),
                Pt2D::new(x2, y1),
                Pt2D::new(x2, y2),
                Pt2D::new(x1, y2),
                Pt2D::new(x1, y1),
            ])
            .to_outline(Distance::meters(20.0)),
        );
        g.disable_clipping();
        g.unfork();
    }
}
