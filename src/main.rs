mod app;
mod image_loader;
mod pane;
mod split_tree;
mod ui;

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("f2viewer"),
        ..Default::default()
    };

    eframe::run_native(
        "f2viewer",
        options,
        Box::new(|cc| Ok(Box::new(app::F2ViewerApp::new(cc)))),
    )
}
