use eframe::{egui::{self, Color32, CornerRadius, Frame, Margin, Pos2, Rect, Shadow, Stroke, UiBuilder, Vec2}, NativeOptions};

struct App {

}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Window::new("thiz iz my window")
            .frame(Frame {
                inner_margin: Margin::ZERO,
                fill: Color32::from_gray(0xD5),
                stroke: Stroke::new(1., Color32::BLACK),
                corner_radius: CornerRadius::ZERO,
                outer_margin: Margin::ZERO,
                shadow: Shadow::NONE
            })
            .title_bar(false)
            .resizable(false)
            .show(ctx, |ui| {

            let mut inner_rect = ui.cursor().shrink(5.);
            inner_rect.min.y += 16.;
            
            let resp = ui.allocate_new_ui(
                UiBuilder::new().max_rect(inner_rect),
                |ui| {
                    ui.label("hello world");
                    ui.label("may i take your order");
                }
            );
            
            let mut rect = resp.response.rect;
            rect.max += Vec2::splat(5.);
            ui.expand_to_include_rect(rect);
            
            let painter = ui.painter();
            let rect = ui.min_rect().shrink(0.5);
            painter.vline(rect.min.x, rect.min.y-0.5..=rect.max.y, (1., Color32::WHITE));
            painter.hline(rect.min.x..=rect.max.x, rect.min.y, (1., Color32::WHITE));
            painter.vline(rect.max.x, rect.min.y..=rect.max.y+0.5, (1., Color32::from_gray(0xA7)));
            painter.hline(rect.min.x..=rect.max.x, rect.max.y, (1., Color32::from_gray(0xA7)));

            painter.rect_filled(Rect::from_center_size(Pos2::new(rect.max.x, rect.min.y), Vec2::splat(1.)), 0, Color32::from_gray(0xD5));
            painter.rect_filled(Rect::from_center_size(Pos2::new(rect.min.x, rect.max.y), Vec2::splat(1.)), 0, Color32::from_gray(0xD5));

            let outer_stroke_rect = resp.response.rect.expand(1.);
            painter.rect_stroke(outer_stroke_rect, 0, (1., Color32::BLACK), egui::StrokeKind::Inside);
            // painter.rect_stroke(outer_stroke_rect, 0, (1., Color32::from_gray(0xA7)), egui::StrokeKind::Inside);

            painter.vline(outer_stroke_rect.min.x-0.5, outer_stroke_rect.min.y-1.0..=outer_stroke_rect.max.y, (1., Color32::from_gray(0xA7)));
            painter.hline(outer_stroke_rect.min.x-1.0..=outer_stroke_rect.max.x-0.5, outer_stroke_rect.min.y-0.5, (1., Color32::from_gray(0xA7)));

            painter.vline(outer_stroke_rect.max.x+0.5, outer_stroke_rect.min.y..=outer_stroke_rect.max.y+1.0, (1., Color32::WHITE));
            painter.hline(outer_stroke_rect.min.x..=outer_stroke_rect.max.x+0.5, outer_stroke_rect.max.y+0.5, (1., Color32::WHITE));
        });

        egui::CentralPanel::default().frame(Frame {
            inner_margin: Margin::ZERO,
            fill: Color32::WHITE,
            stroke: Stroke::NONE,
            corner_radius: CornerRadius::ZERO,
            outer_margin: Margin::ZERO,
            shadow: Shadow::NONE
        }).show(ctx, |_| ());
    }
}

fn main() {
    eframe::run_native(
        "net.sohio.ninewire.ui",
        NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        }, Box::new(|ctx| {
            ctx.egui_ctx.style_mut(|st| {
                st.visuals.override_text_color = Some(Color32::BLACK);
                st.visuals.window_corner_radius = CornerRadius::ZERO;
            });
            Ok(Box::new(App {}))}
        )).unwrap();
}
