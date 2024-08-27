use crate::prelude::*;
use tracing_log::LogTracer;
use tracing_subscriber::{prelude::*, registry::Registry, EnvFilter};

#[derive(Default)]
pub struct LogPlugin;

/// `LogPlugin` settings
#[derive(Resource)]
pub struct LogSettings {
    /// Filters logs using the [`EnvFilter`] format
    pub filter: String,

    /// Filters out logs that are "less than" the given level.
    /// This can be further filtered using the `filter` setting.
    pub level: Level,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            filter: "wgpu=error".to_string(),
            level: Level::INFO,
        }
    }
}

impl Plugin for LogPlugin {
    fn build(&self, app: &mut App) {
        let default_filter = {
            let settings = app
                .world_mut()
                .get_resource_or_insert_with(LogSettings::default);
            format!("{},{}", settings.level, settings.filter)
        };
        LogTracer::init().unwrap();
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&default_filter))
            .unwrap();
        let subscriber = Registry::default().with(filter_layer);

        // Allow us to output our logging for quick diffing.
        // e.g., `cargo run > log1.log` and `cargo run > log2.log`
        let fmt_layer = tracing_subscriber::fmt::Layer::default()
            .without_time()
            .with_target(false)
            .with_level(false)
            .with_ansi(false);

        let subscriber = subscriber.with(fmt_layer);

        bevy::utils::tracing::subscriber::set_global_default(subscriber)
                .expect("Could not set global default tracing subscriber. If you've already set up a tracing subscriber, please disable LogPlugin from Bevy's DefaultPlugins");
    }
}
