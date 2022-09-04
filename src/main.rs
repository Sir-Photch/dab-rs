mod chimes;
mod handler;
mod fluent;

use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration
};
use log::{info, error};
use chrono::prelude::*;
use config::Config;
use serenity::{
    prelude::*,
    framework::standard::StandardFramework
};
use songbird::SerenityInit;

fn setup_logger() -> Result<(), fern::InitError> {

    let colors = fern::colors::ColoredLevelConfig::new()
        .error(fern::colors::Color::BrightRed);

    let file_config = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}][{}] | {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .level_for("serenity", log::LevelFilter::Warn)
        .level_for("songbird", log::LevelFilter::Warn)
        .level_for("tracing", log::LevelFilter::Warn)
        .chain(fern::log_file("activity.log")?);
    
    let stderr_config = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} | [{}][{}] -- {}",
                Local::now().format("%H:%M:%S"),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Error)
        .chain(std::io::stderr());

    fern::Dispatch::new()
        .chain(file_config)
        .chain(stderr_config)
        .apply()?;

    Ok(())
}

#[tokio::main]
async fn main() {
    setup_logger().expect("Could not setup logger!");

    let settings = Config::builder()
        .add_source(config::File::with_name("Settings"))
        .build()
        .expect("Could not build settings!")
        .try_deserialize::<HashMap<String, String>>()
        .expect("Could not deserialize settings!");

    let framework = StandardFramework::new();
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_VOICE_STATES;

    let mut userdata_dir = std::path::PathBuf::new();
    userdata_dir.push(settings["USERDATA_DIR"].as_str());

    let sink = chimes::FileChimeSink::new(
        userdata_dir
    ).await.expect("Could not initialize sink!");

    let sink = Arc::new(sink);

    let mut client = Client::builder(settings["API_TOKEN"].as_str(), intents)
        .event_handler(
            handler::Handler::new(
                sink, 
                settings["BUS_SIZE"].as_str()
                                            .parse::<usize>()
                                            .expect("Could not get bus-size from config"),
                Duration::from_millis(
                    settings["CHIME_DURATION_MAX_MS"].as_str()
                                                     .parse::<u64>()
                                                     .expect("Could not get file-duration-max from config")
                ),
                1000 * settings["FILE_SIZE_LIMIT_KILOBYTES"].as_str()
                                                                               .parse::<i64>()
                                                                               .expect("Could not get maximum filesize from config"),
                settings["COMMAND_ROOT"].clone(),
                Duration::from_millis(
                    settings["CONNECTION_TIMEOUT_MILLISECONDS"].as_str()
                                                               .parse::<u64>()
                                                               .expect("Could not get connection-timeout-ms from config")
                )
            )
        )
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    let exec_start = Utc::now();

    tokio::spawn(async move {
        let _ = client.start().await.map_err(|why| error!("Client ended: {:#?}", why));
    });

    let _ = tokio::signal::ctrl_c().await;
    info!("Received interrupt. Session lasted {}. Exiting...", Utc::now() - exec_start);
}
