use crate::*;

use serenity::async_trait;
use serenity::http::CacheHttp;
use serenity::prelude::*;
use serenity::model::{
    gateway::Ready,
    voice::VoiceState,
    application::{
        command::{Command, CommandOptionType},
        interaction::{            
            Interaction,
            InteractionResponseType,
            application_command::CommandDataOptionValue
        }
    }
};
use songbird::input::Input;
use std::collections::{HashSet, VecDeque};
use log::{info, warn, error};

pub struct Handler {
    sink : Box<dyn chimes::ChimeSink>,
    queues : Mutex<
        HashMap<u64, // guilds
            Mutex<HashMap<u64, VecDeque<Input>>> // channels
            >
        >,
    active_guilds : Mutex<HashSet<u64>>
}
impl Handler {
    pub fn new(sink: Box<dyn chimes::ChimeSink>) -> Self {
        Self 
        { 
            sink, 
            queues : Mutex::new(HashMap::new()), // always one queue for all guilds served
            active_guilds : Mutex::new(HashSet::new())
        }
    }
}
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            info!("Received command interaction: {:#?}", command);

            let name: &str = command.data.name.as_str();
            if name != "chime" {
                warn!("Unknown command received! {:#?}", name);
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
        }
    }

    async fn voice_state_update(&self, ctx: Context, old: Option<serenity::model::voice::VoiceState>, new: serenity::model::voice::VoiceState) {
        use tokio::task;

        let user = new.user_id.to_user(&ctx.http).await;
        if user.is_err() {
            error!("Unexpected error, user could not be retrieved! {:#?}", user);
            return
        }

        let user = user.unwrap();

        if user.bot {
            return
        }

        if new.channel_id == None {
            return
        }

        let channel_id = new.channel_id.unwrap();

        if let Some(prev) = old.and_then(|state| state.channel_id) {
            if prev == channel_id {
                return
            }
        }

        if !self.sink.has_data(user.id.0).await {
            return;
        }

        if new.guild_id == None {
            warn!("Unexpected: user connected to unknown guild");
            return
        }
        let guild_id = new.guild_id.unwrap();

        let chime = self.sink.get_input(user.id.0).await;

        if chime.is_err() {
            error!("Could not retrieve chime from sink {:#?}", chime);
            return
        }

        let chime = chime.unwrap();
        {
            self.queues
                .lock()
                .await
                .entry(*guild_id.as_u64())
                .or_insert(
            Mutex::new(
                        HashMap::new()
                    )
                ).lock()
                .await
                .entry(channel_id.0)
                .or_insert(VecDeque::new())
                .push_back(chime); 
        } 
            
        let active_guilds = self.active_guilds.lock();
        let guild_map = self.queues.lock().await;

        let channel_map = guild_map[guild_id.as_u64()].lock();

        drop(guild_map);

        task::spawn(async {
            if !active_guilds.await.insert(guild_id.0) {
                return
            }

            channel_map.await;

            if !active_guilds.await.remove(&guild_id.0) {
                warn!("There is something spooky going on!");
            }

            info!("Exited task.");
        });
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        use serenity::model::{
            gateway::Activity,
            user::OnlineStatus,
            prelude::command::{Command, CommandOptionType},
        };

        ctx.set_presence(Some(Activity::listening("/chime")), OnlineStatus::Online).await;

        info!("{} is connected!", ready.user.name);

        Command::set_global_application_commands(&ctx.http, |create_app_commands| {
            create_app_commands.create_application_command(|cmd| {
                cmd.name("chime")
                    .description("modify your chime")
                    .description_localized("de", "Willkommenssound anpassen")
                    .create_option(|opt| {
                        opt.name("clear")
                            .description("clear your chime")
                            .description_localized("de", "Entfernt deinen Willkommenssound")
                            .kind(CommandOptionType::SubCommand)
                    })
                    .create_option(|opt| {
                        opt.name("set")
                            .description("set your chime")
                            .description_localized("de", "Setzt deinen Willkommenssound")
                            .kind(CommandOptionType::SubCommand)
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::Attachment)
                                    .name("file")
                                    .required(true)
                                    .description("attachment with file")
                                    .description_localized("de", "Anhang mit Datei")
                            })
                            .create_sub_option(|opt| {
                                opt.kind(CommandOptionType::String)
                                    .name("url")
                                    .required(true)
                                    .description("link to file")
                                    .description_localized("de", "Link zur Datei")
                            })
                    })
            })
        })
        .await
        .expect("could not set commands!");
    }
}