extern crate getopts;
mod chimes;
mod data;
mod fluent;
mod handler;
mod localizable;
mod nameable;

use chrono::prelude::*;
use config::Config;
use getopts::Options;
use log::{error, info};
use serenity::prelude::*;
use songbird::SerenityInit;
use std::{collections::HashMap, env, sync::Arc, time::Duration};
use unic_langid::LanguageIdentifier;

fn setup_logger(verbose: bool) -> Result<(), fern::InitError> {
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

    let stdout_config = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}][{}] -- {}",
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(if verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Warn
        })
        .level_for("tracing", log::LevelFilter::Warn)
        .chain(std::io::stdout());

    fern::Dispatch::new()
        .chain(file_config)
        .chain(stdout_config)
        .apply()?;

    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut opts = Options::new();
    opts.optflag("v", "verbose", "Verbose logging in stdout");
    let verbose = opts
        .parse(&args[1..])
        .expect("Bad arguments!")
        .opt_present("v");

    setup_logger(verbose).expect("Could not setup logger!");

    let settings = Config::builder()
        .add_source(config::File::with_name("Settings"))
        .build()
        .expect("Could not build settings!")
        .try_deserialize::<HashMap<String, String>>()
        .expect("Could not deserialize settings!");

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

    let mut config = tokio_postgres::config::Config::new();
    config
        .host(settings["DB_HOSTNAME"].as_str())
        .user(settings["DB_USERNAME"].as_str())
        .password(settings["DB_PASSWORD"].as_str())
        .dbname(settings["DB_NAME"].as_str());

    let (client, connection) = config
        .connect(tokio_postgres::NoTls)
        .await
        .expect("Bad database config");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("Could not connect to database: {e:?}");
        }
    });

    let database_interface = data::DatabaseInterface::new(client, settings["DB_TABLE"].to_owned());

    database_interface.ensure_table_exists().await;

    let sink = Arc::new(sink);

    let handler = handler::HandlerBuilder::default()
        .command_root(&settings["COMMAND_ROOT"])
        .localizer(localizer)
        .database(database_interface)
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
        .register_songbird()
        .await
        .expect("Error creating client");

    let exec_start = Utc::now();

    let client_handle = tokio::spawn(async move {
        client
            .start()
            .await
            .map_err(|err| error!("Client error: {err:?}"))
    });

    if let Err(why) = tokio::signal::ctrl_c().await {
        error!("ctrl-c error: {why:?}");
    }

    info!(
        "Received interrupt. Session lasted {}. Exiting...",
        Utc::now() - exec_start
    );

    client_handle.abort();
    if let Err(why) = client_handle.await {
        if why.is_panic() {
            error!("==> Client task panicked: {why:?}");
        }
    }
}
