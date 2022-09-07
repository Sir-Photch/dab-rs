mod chimes;
mod data;
mod fluent;
mod handler;

use chrono::prelude::*;
use config::Config;
use log::{error, info};
use serenity::{framework::standard::StandardFramework, prelude::*};
use songbird::SerenityInit;
use std::{collections::HashMap, sync::Arc, time::Duration};
use unic_langid::LanguageIdentifier;

fn setup_logger() -> Result<(), fern::InitError> {
    let colors = fern::colors::ColoredLevelConfig::new().error(fern::colors::Color::BrightRed);

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
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;

    let mut userdata_dir = std::path::PathBuf::new();
    userdata_dir.push(settings["USERDATA_DIR"].as_str());

    let sink = chimes::FileChimeSink::new(userdata_dir)
        .await
        .expect("Could not initialize sink!");

    let mut resource_dir = std::path::PathBuf::new();
    resource_dir.push(settings["RESOURCE_DIR"].as_str());

    let localizer = fluent::FluentLocalizer::new(
        settings["DEFAULT_LOCALE"]
            .parse::<LanguageIdentifier>()
            .expect("Could not parse default locale!"),
        resource_dir,
    )
    .expect("Could not initialize localizer!");

    let db_opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(settings["DB_HOSTNAME"].as_str())
        .user(Some(settings["DB_USERNAME"].as_str()))
        .pass(Some(settings["DB_PASSWORD"].as_str()))
        .db_name(Some(settings["DB_NAME"].as_str()));

    let database_interface = data::DatabaseInterface::new(mysql_async::Pool::new(db_opts));

    database_interface
        .ensure_table_exists(settings["DB_TABLE"].as_str())
        .await;

    let sink = Arc::new(sink);

    let handler = handler::HandlerBuilder::default()
        .command_root(&settings["COMMAND_ROOT"])
        .localizer(localizer)
        .database(database_interface.clone())
        .sink(sink)
        .bus_size(
            settings["BUS_SIZE"]
                .as_str()
                .parse::<usize>()
                .expect("Could not get bus-size from config"),
        )
        .file_size_limit(
            1000 * settings["FILE_SIZE_LIMIT_KILOBYTES"]
                .as_str()
                .parse::<isize>()
                .expect("Could not get maximum filesize from config"),
        )
        .file_duration_max(Duration::from_millis(
            settings["CHIME_DURATION_MAX_MS"]
                .as_str()
                .parse::<u64>()
                .expect("Could not get file-duration-max from config"),
        ))
        .disconnect_timeout(Duration::from_millis(
            settings["CONNECTION_TIMEOUT_MILLISECONDS"]
                .as_str()
                .parse::<u64>()
                .expect("Could not get connection-timeout-ms from config"),
        ))
        .build();

    let mut client = Client::builder(settings["API_TOKEN"].as_str(), intents)
        .event_handler(handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    let exec_start = Utc::now();

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| error!("Client ended: {:#?}", why));
    });

    let _ = tokio::signal::ctrl_c().await;
    info!(
        "Received interrupt. Session lasted {}. Exiting...",
        Utc::now() - exec_start
    );

    if let Err(why) = database_interface.disconnect().await {
        error!("Could not disconnect database pool: {why:?}");
    }
}
