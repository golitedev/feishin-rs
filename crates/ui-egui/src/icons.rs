//! Small original line drawings. These are not copied from an external icon set.
use egui::{Color32, Painter, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Home,
    Music,
    Disc,
    Artist,
    Settings,
    Menu,
    Back,
    Down,
    Refresh,
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    Shuffle,
    Repeat,
    Queue,
    Heart,
    Volume,
    Lyrics,
    Close,
    Forward,
    Folder,
    Search,
    Flag,
    Sliders,
    Sidebar,
    Star,
    More,
    Clock,
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, active: bool, primary: bool) -> Response {
    let size = if primary { 34.0 } else { 28.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if primary {
        ui.painter()
            .circle_filled(rect.center(), size / 2.0, Color32::from_rgb(226, 226, 226));
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_rgb(34, 34, 34));
    }
    let color = if primary {
        Color32::BLACK
    } else if active {
        ui.visuals().selection.bg_fill
    } else {
        ui.visuals().text_color()
    };
    draw(
        ui.painter(),
        icon,
        rect.shrink(if primary { 10.0 } else { 7.0 }),
        color,
    );
    response.on_hover_text(label)
}
pub fn paint_icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw(ui.painter(), icon, rect, color);
}
pub fn draw(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let stroke = Stroke::new((rect.width() / 13.0).clamp(1.0, 2.0), color);
    let point = |x: f32, y: f32| {
        pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    let path = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|(x, y)| point(*x, *y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Clock => {
            p.circle_stroke(rect.center(), rect.width() * 0.43, stroke);
            path(&[(0.5, 0.2), (0.5, 0.5), (0.72, 0.5)]);
        }
        Icon::More => {
            for x in [0.2, 0.5, 0.8] {
                p.circle_filled(point(x, 0.5), rect.width() * 0.055, color);
            }
        }
        Icon::Star => {
            let points = (0..=10)
                .map(|index| {
                    let angle =
                        index as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                    let radius = if index % 2 == 0 { 0.48 } else { 0.22 };
                    point(0.5 + angle.cos() * radius, 0.5 + angle.sin() * radius)
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
        }
        Icon::Search => {
            p.circle_stroke(point(0.4, 0.4), rect.width() * 0.28, stroke);
            line((0.61, 0.61), (0.94, 0.94));
        }
        Icon::Flag => {
            path(&[
                (0.12, 0.95),
                (0.12, 0.1),
                (0.9, 0.1),
                (0.73, 0.4),
                (0.9, 0.68),
                (0.12, 0.68),
            ]);
        }
        Icon::Sliders => {
            for (x, y) in [(0.18, 0.3), (0.5, 0.7), (0.82, 0.4)] {
                line((x, 0.05), (x, 0.95));
                line((x - 0.12, y), (x + 0.12, y));
            }
        }
        Icon::Sidebar => {
            p.rect_stroke(
                Rect::from_min_max(point(0.1, 0.1), point(0.9, 0.9)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((0.4, 0.1), (0.4, 0.9));
            path(&[(0.72, 0.35), (0.56, 0.5), (0.72, 0.65)]);
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![point(0.2, 0.05), point(0.95, 0.5), point(0.2, 0.95)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            p.rect_filled(
                Rect::from_min_max(point(0.14, 0.05), point(0.37, 0.95)),
                0.0,
                color,
            );
            p.rect_filled(
                Rect::from_min_max(point(0.63, 0.05), point(0.86, 0.95)),
                0.0,
                color,
            );
        }
        Icon::Stop => {
            p.rect_filled(
                Rect::from_min_max(point(0.15, 0.15), point(0.85, 0.85)),
                1.0,
                color,
            );
        }
        Icon::Next | Icon::Previous => {
            let flip = matches!(icon, Icon::Previous);
            let q = |x: f32, y: f32| point(if flip { 1.0 - x } else { x }, y);
            p.add(egui::Shape::convex_polygon(
                vec![q(0.1, 0.1), q(0.75, 0.5), q(0.1, 0.9)],
                color,
                Stroke::NONE,
            ));
            p.line_segment([q(0.9, 0.1), q(0.9, 0.9)], stroke);
        }
        Icon::Menu => {
            for y in [0.25, 0.5, 0.75] {
                line((0.1, y), (0.9, y));
            }
        }
        Icon::Forward => {
            path(&[(0.35, 0.12), (0.7, 0.5), (0.35, 0.88)]);
        }
        Icon::Folder => {
            path(&[
                (0.05, 0.2),
                (0.4, 0.2),
                (0.5, 0.32),
                (0.95, 0.32),
                (0.95, 0.85),
                (0.05, 0.85),
                (0.05, 0.2),
            ]);
        }
        Icon::Back => {
            path(&[(0.65, 0.12), (0.3, 0.5), (0.65, 0.88)]);
        }
        Icon::Down => {
            path(&[(0.12, 0.3), (0.5, 0.7), (0.88, 0.3)]);
        }
        Icon::Close => {
            line((0.2, 0.2), (0.8, 0.8));
            line((0.8, 0.2), (0.2, 0.8));
        }
        Icon::Home => {
            path(&[
                (0.1, 0.45),
                (0.5, 0.1),
                (0.9, 0.45),
                (0.9, 0.9),
                (0.1, 0.9),
                (0.1, 0.45),
            ]);
            line((0.3, 0.67), (0.7, 0.67));
        }
        Icon::Music => {
            path(&[(0.3, 0.75), (0.3, 0.15), (0.85, 0.05), (0.85, 0.65)]);
            p.circle_stroke(point(0.18, 0.8), rect.width() * 0.13, stroke);
            p.circle_stroke(point(0.73, 0.7), rect.width() * 0.13, stroke);
        }
        Icon::Disc => {
            p.circle_stroke(rect.center(), rect.width() * 0.43, stroke);
            p.circle_stroke(rect.center(), rect.width() * 0.16, stroke);
            p.circle_filled(rect.center(), rect.width() * 0.035, color);
        }
        Icon::Artist => {
            p.circle_stroke(point(0.42, 0.28), rect.width() * 0.20, stroke);
            path(&[
                (0.05, 0.95),
                (0.12, 0.7),
                (0.3, 0.57),
                (0.55, 0.57),
                (0.75, 0.7),
                (0.8, 0.95),
            ]);
            path(&[(0.82, 0.1), (0.92, 0.25), (0.92, 0.45), (0.82, 0.6)]);
        }
        Icon::Settings => {
            p.circle_stroke(rect.center(), rect.width() * 0.25, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let v = vec2(a.cos(), a.sin());
                p.line_segment(
                    [
                        rect.center() + v * rect.width() * 0.29,
                        rect.center() + v * rect.width() * 0.45,
                    ],
                    stroke,
                );
            }
        }
        Icon::Refresh => {
            path(&[
                (0.84, 0.3),
                (0.68, 0.12),
                (0.35, 0.12),
                (0.1, 0.4),
                (0.1, 0.7),
                (0.35, 0.9),
                (0.68, 0.9),
                (0.9, 0.7),
            ]);
            path(&[(0.88, 0.06), (0.88, 0.35), (0.6, 0.35)]);
        }
        Icon::Shuffle => {
            path(&[(0.05, 0.2), (0.25, 0.2), (0.74, 0.8), (0.95, 0.8)]);
            path(&[(0.05, 0.8), (0.25, 0.8), (0.74, 0.2), (0.95, 0.2)]);
            path(&[(0.8, 0.05), (0.95, 0.2), (0.8, 0.35)]);
            path(&[(0.8, 0.65), (0.95, 0.8), (0.8, 0.95)]);
        }
        Icon::Repeat => {
            path(&[(0.2, 0.2), (0.9, 0.2), (0.9, 0.65)]);
            path(&[(0.8, 0.8), (0.1, 0.8), (0.1, 0.35)]);
            path(&[(0.35, 0.05), (0.2, 0.2), (0.35, 0.35)]);
            path(&[(0.65, 0.65), (0.8, 0.8), (0.65, 0.95)]);
        }
        Icon::Queue => {
            for y in [0.2, 0.45, 0.7] {
                line((0.08, y), (0.75, y));
            }
            line((0.8, 0.65), (0.8, 0.98));
            line((0.63, 0.82), (0.97, 0.82));
        }
        Icon::Heart => {
            path(&[
                (0.5, 0.9),
                (0.12, 0.55),
                (0.03, 0.3),
                (0.15, 0.12),
                (0.35, 0.1),
                (0.5, 0.26),
                (0.65, 0.1),
                (0.85, 0.12),
                (0.97, 0.3),
                (0.88, 0.55),
                (0.5, 0.9),
            ]);
        }
        Icon::Volume => {
            path(&[
                (0.1, 0.35),
                (0.3, 0.35),
                (0.6, 0.1),
                (0.6, 0.9),
                (0.3, 0.65),
                (0.1, 0.65),
                (0.1, 0.35),
            ]);
            path(&[(0.78, 0.28), (0.9, 0.43), (0.9, 0.57), (0.78, 0.72)]);
        }
        Icon::Lyrics => {
            path(&[
                (0.35, 0.7),
                (0.2, 0.85),
                (0.35, 1.0),
                (0.85, 0.48),
                (0.7, 0.32),
                (0.35, 0.7),
            ]);
            p.circle_stroke(point(0.75, 0.25), rect.width() * 0.19, stroke);
        }
    }
}

pub fn toolbar_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(46.0, 36.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        2.0,
        if response.hovered() {
            ui.visuals().widgets.hovered.weak_bg_fill
        } else {
            ui.visuals().extreme_bg_color
        },
    );
    draw(
        ui.painter(),
        icon,
        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
        ui.visuals().text_color(),
    );
    response.on_hover_text(label)
}
