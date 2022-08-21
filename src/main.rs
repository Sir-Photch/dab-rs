use config::Config;
use log::{error, info, warn};
use mysql_async::prelude::*;
use serenity::async_trait;
use serenity::framework::standard::StandardFramework;
use serenity::model::application::interaction::application_command::CommandDataOptionValue;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::id::GuildId;
use serenity::model::prelude::Ready;
use serenity::prelude::*;
use std::collections::HashMap;
use std::error::Error;
use url::{ParseError, Url};

#[derive(Debug, PartialEq, Eq)]
struct Playlist {
    guild_id: u64,
    name: String,
    entry_author: u64,
    url: Url,
}

fn generate_list(entries: &Vec<Playlist>) -> String {
    let str = String::with_capacity(entries.len());

    for entry in entries {}

    return str;
}

struct General;

struct Handler {
    sql_conn: Mutex<mysql_async::Conn>,
}
impl Drop for Handler {
    fn drop(&mut self) {
        drop(&self.sql_conn);
    }
}
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            info!("Received command interaction: {:#?}", command);

            let guild = match command.guild_id {
                Some(id) => id.0,
                None => {
                    command
                        .create_interaction_response(&ctx.http, |r| {
                            r.kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|d| {
                                    d.content("This only works in guilds!")
                                })
                        })
                        .await;
                    return;
                }
            };

            let name: &str = command.data.name.as_str();
            if name != "playlist" {
                warn!("Unknown command received! {:?}", name);
                return;
            }

            let user: u64 = command.user.id.0;

            // playlist list
            //
            // playlist add name
            // playlist remove name
            // playlist play name
            let base_option = command
                .data
                .options
                .get(0);

            if let None = base_option {
                error!("Bad base option in command!");
                return
            }

            let base_option = base_option.unwrap();

            match base_option.name.as_str() {
                "list" => {
                    match self
                        .sql_conn
                        .lock()
                        .await
                        .query_map(
                            format!(
                                r"SELECT name, entry_author, url 
                                FROM lists 
                                WHERE guild_id = {}",
                                guild
                            ),
                            |(name, entry_author, url_str): (String, String, String)| {
                                let url = Url::parse(&url_str).expect("bad url in database!");
                                let entry_author = entry_author.parse::<u64>().unwrap();
                                let guild_id = guild;
                                let name = name.to_string();

                                Playlist {
                                    guild_id,
                                    name,
                                    entry_author,
                                    url,
                                }
                            },
                        )
                        .await
                    {
                        Ok(list) => {
                            if list.len() == 0 {
                                return;
                            }

                            let mut list_string = String::new();

                            for item in list {
                                list_string.push_str(&format!("{}, {}\n", item.name, item.url))
                            }

                            command
                                .create_interaction_response(&ctx.http, |response| {
                                    response
                                        .kind(InteractionResponseType::ChannelMessageWithSource)
                                        .interaction_response_data(|data| data.content(list_string))
                                })
                                .await;

                            // TODO create nice list
                        }
                        Err(why) => {
                            error!("Could not get list of playlists from database! {}", why);
                        }
                    };
                }
                "add" => {
                    let mut name_url = (Option::<String>::None, Option::<Url>::None);

                    for item in &base_option.options {
                        match item.name.as_str() {
                            "name" => {
                                if let Some(CommandDataOptionValue::String(value)) = &item.resolved {
                                    name_url.0 = Some(value.to_owned());
                                }
                            },
                            "url" => {
                                if let Some(CommandDataOptionValue::String(value)) = &item.resolved {
                                    if let Ok(url) = Url::parse(value.as_str()) {
                                        name_url.1 = Some(url);
                                    } else {
                                        error!("Could not parse url {}", value);
                                    }
                                }
                            },
                            _ => {
                                error!("Unexpected token in command!");
                            }
                        }
                    }

                    if name_url.0 == Option::None || name_url.1 == Option::None {
                        return
                    }

                    let playlist_name = name_url.0.unwrap().to_owned();
                    let playlist_url = name_url.1.unwrap().to_owned();

                    info!("Adding {} {} {} {} into database", guild, playlist_url, user, playlist_name);

                    match self.sql_conn.lock().await.query_drop(format!(
                        r"INSERT INTO lists (guild_id, name, entry_author, url)
                          VALUES ({},'{}',{},'{}')",
                        guild, playlist_name, user, playlist_url
                    )).await
                    {
                        Ok(()) => {
                            command.create_interaction_response(&ctx.http, |r|{
                                r.kind(InteractionResponseType::ChannelMessageWithSource)
                                 .interaction_response_data(|d| d.content("success!"))
                            }).await;
                        },
                        Err(why) => {
                            error!("Could not add entry to database {:?}", why);
                        }
                    }
                }
                "remove" => {
                    let list_to_remove = base_option.options.get(0);

                    if let Some(CommandDataOptionValue::String(value)) = list_to_remove.resolved {
                        match self.sql_conn.lock().await.query_drop(format!(
                            r"DELETE FROM lists WHERE guild_id = {} AND name = '{}'",
                            guild, value
                        )).await 
                        {
                            Ok(()) => {
                                command.create_interaction_response(&ctx.http, |r|{
                                    r.kind(InteractionResponseType::ChannelMessageWithSource)
                                     .interaction_response_data(|d| d.content("success!"))
                                }).await;
                            },
                            Err(why) => {
                                error!("Could not remove entriy from database! {:?}", why);
                            }
                        }
                    }
                }
                "play" => {
                    // TODO
                }
                value => {
                    warn!("Unknown command option received {:?}", value);
                }
            };
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        use serenity::model::prelude::command::{Command, CommandOptionType};

        info!("{} is connected!", ready.user.name);

        Command::set_global_application_commands(&ctx.http, |create_app_commands| {
            create_app_commands.create_application_command(|cmd| {
                cmd.name("playlist")
                    .description("manage playlists")
                    .description_localized("de", "Playlistverwaltung")
                    .create_option(|opt| {
                        opt.name("list")
                            .description("list available playlists")
                            .description_localized("de", "Gespeicherte Playlisten anzeigen")
                            .kind(CommandOptionType::SubCommand)
                    })
                    .create_option(|opt| {
                        opt.name("add")
                            .description("add playlist to list of playlists")
                            .description_localized("de", "Playlist-Link in die Liste hinzufügen")
                            .kind(CommandOptionType::SubCommand)
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::String)
                                    .name("name")
                                    .required(true)
                                    .description("name of the playlist")
                                    .description_localized("de", "Name der Plalist")
                            })
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::String)
                                    .name("url")
                                    .required(true)
                                    .description("link to playlist")
                                    .description_localized("de", "Link zur Playlist")
                            })
                    })
                    .create_option(|opt| {
                        opt.name("remove")
                            .description("remove playlists from list of playlists")
                            .description_localized("de", "Playlist-Link aus der Liste entfernen")
                            .kind(CommandOptionType::SubCommand)
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::String)
                                    .name("name")
                                    .required(true)
                                    .description("name of the playlist")
                                    .description_localized("de", "Name der Plalist")
                            })
                    })
                    .create_option(|opt| {
                        opt.name("play")
                            .description("play playlist based on given name")
                            .description_localized("de", "Playlist anhand vom Namen abspielen")
                            .kind(CommandOptionType::SubCommand)
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::String)
                                    .name("name")
                                    .required(true)
                                    .description("name of the playlist")
                                    .description_localized("de", "Name der Plalist")
                            })
                    })
            })
        })
        .await
        .expect("could not set commands!");
    }
}

#[tokio::main]
async fn main() {
    stderrlog::new().module(module_path!()).init().unwrap();

    let settings = Config::builder()
        .add_source(config::File::with_name("Settings"))
        .build()
        .expect("Could not build settings!")
        .try_deserialize::<HashMap<String, String>>()
        .expect("Could not deserialize settings!");

    let pool = mysql_async::Pool::new(settings["DB_URL"].as_str());
    let mut conn = pool
        .get_conn()
        .await
        .expect("Could not connect to database!");

    if let Err(why) = r"CREATE TABLE IF NOT EXISTS lists (
        guild_id bigint unsigned not null,
        name varchar(32) not null,        
        entry_author bigint unsigned not null,
        url text not null,        
        primary key (guild_id, name)
    )"
    .ignore(&mut conn)
    .await
    {
        println!("Fatal, could not ensure database table {:?}", why);
        return;
    }

    let framework = StandardFramework::new();
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let db_mutex = Mutex::new(conn);

    let mut client = Client::builder(settings["API_TOKEN"].as_str(), intents)
        .event_handler(Handler { sql_conn: db_mutex })
        .framework(framework)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("An error occurred: {:?}", why);
    }

    pool.disconnect()
        .await
        .expect("Could not disconnect from pool");
}
