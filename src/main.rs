use anyhow::anyhow;
use eframe::{NativeOptions, Theme};
use log::LevelFilter;
use simplelog::{ColorChoice, ConfigBuilder, TermLogger, TerminalMode};
use ui::Application;

mod bing;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    TermLogger::init(
        LevelFilter::Trace,
        ConfigBuilder::new().add_filter_allow_str("bing").build(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("failed to init logger");

    eframe::run_native(
        "Bing Client",
        NativeOptions {
            follow_system_theme: true,
            default_theme: Theme::Light,
            ..Default::default()
        },
        Box::new(|cc| Box::new(Application::new(cc))),
    )
    .map_err(|e| anyhow!("failed to run eframe: {}", e))
}
