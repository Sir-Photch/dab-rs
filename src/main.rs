mod chimes;
mod handler;

use config::Config;
use serenity::framework::standard::StandardFramework;
use serenity::prelude::*;
use std::collections::HashMap;

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
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut userdata_dir = std::path::PathBuf::new();
    userdata_dir.push(settings["USERDATA_DIR"].as_str());

    let sink = chimes::FileChimeSink::new(
        userdata_dir, 
        settings["DEBUG"].as_str().parse::<bool>().expect("Bad config")
    ).await.expect("Could not initialize sink!");

    let mut client = Client::builder(settings["API_TOKEN"].as_str(), intents)
        .event_handler(handler::Handler::new(Box::new(sink)))
        .framework(framework)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("An error occurred: {:?}", why);
    }
}
