mod chimes;
mod handler;

use std::{
    collections::HashMap,
    sync::Arc
};
use log::{info, error};
use config::Config;
use serenity::{
    prelude::*,
    framework::standard::StandardFramework
};
use songbird::SerenityInit;

#[tokio::main]
async fn main() {
    env_logger::init();

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
                                            .expect("Could not get bus-size from config")
            )
        )
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    tokio::spawn(async move {
        let _ = client.start().await.map_err(|why| error!("Client ended: {:#?}", why));
    });

    let _ = tokio::signal::ctrl_c().await;
    info!("Received interrupt. Exiting...");
}
