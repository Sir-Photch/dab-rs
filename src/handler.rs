use crate::*;

use std::{
    sync::Arc,
    collections::{HashSet, VecDeque}
};
use serenity::{
    async_trait, 
    prelude::*, 
    http::CacheHttp, 
    model::{
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
}};
use songbird::input::Input;
use log::{info, warn, error};

pub struct Handler {
    sink : Box<dyn chimes::ChimeSink>,

    // send help
    queues : Arc<Mutex<
        HashMap<u64, // guilds
            Arc<Mutex<HashMap<u64, 
                Arc<Mutex<VecDeque<Input>>>>>> // channels
            >
        >>,

    active_guilds : Arc<Mutex<HashSet<u64>>>
}
impl Handler {
    pub fn new(sink: Box<dyn chimes::ChimeSink>) -> Self {
        Self 
        { 
            sink, 
            queues : Arc::new(Mutex::new(HashMap::new())), // always one queue for all guilds served
            active_guilds : Arc::new(Mutex::new(HashSet::new()))
        }
    }
}
#[async_trait]
impl EventHandler for Handler {
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
            Arc::new(Mutex::new(
                        HashMap::new()
                    ))
                ).lock()
                .await
                .entry(channel_id.0)
                .or_insert(Arc::new(Mutex::new(VecDeque::new())))
                .lock()
                .await
                .push_back(chime); 
        } 
            
        let active_guilds = self.active_guilds.clone();
        let guild_map = self.queues.clone();

        task::spawn(async move {
            // check if other task is already operating within guild
            if !active_guilds.lock().await.insert(guild_id.0) {
                return
            }

            if let Some (guild_arc) = guild_map.lock().await.get(&guild_id.0) {
                
                let guild_guard = guild_arc.lock().await;
                let channels_to_play = guild_guard.keys();

                if channels_to_play.len() != 0 {

                    let manager = songbird::get(&ctx).await;
                    if manager.is_none() {
                        error!("Could not get songbird!");
                    }
                    let manager = manager.unwrap().clone();

                    for channel in channels_to_play {

                        let chimes = guild_guard.get(channel);
                        if chimes.is_none() {
                            error!("Attempted to join in channel without chimes");
                            continue;
                        }

                        let handler = manager.join(guild_id, *channel).await;
                        if handler.1.is_err() {
                            error!("Could not join channel! {:#?}", handler.1);
                            continue;
                        }
                        
                        let mut handler_play_lock = handler.0.lock().await;

                        while let Some(chime) = chimes.unwrap().clone().lock().await.pop_front() {
                            handler_play_lock.play_source(chime);
                        }
                    }
                }                
            }
            
            if !active_guilds.lock().await.remove(&guild_id.0) {
                // guild has been removed by other task! this should not happen
                warn!("There is something spooky going on!");
            }

            info!("Exited task.");
        });
    }

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
}