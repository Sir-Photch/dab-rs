use crate::*;

use std::{
    sync::Arc,
    fs::File,
    env::temp_dir, 
    os::unix::prelude::FileExt, 
    time::Duration, 
    fmt::Display
};
use tokio::{
    task::{self, JoinHandle},
    sync::Mutex
};
use log::{info, warn, error};
use serenity::{
    async_trait, 
    prelude::*,
    model::{
    gateway::Ready,
    application::{
        interaction::{
            Interaction,
            InteractionResponseType,    
            application_command::CommandDataOption,
            application_command::CommandDataOptionValue,
            application_command::ApplicationCommandInteraction
        }
    }
}};
use ffprobe::ffprobe;

#[derive(Debug)]
enum AttachmentError {
    Duration,
    Unreadable,
    Tempfile
}
impl Display for AttachmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentError::Duration => write!(f, "Duration is too long"),
            AttachmentError::Unreadable => write!(f, "Cant read the chime"),
            AttachmentError::Tempfile => write!(f, "Oops! Internal error"),
        }
    }
}

#[derive(Clone)]
struct BusChimePayload {
    guild_id : u64,
    channel_id : u64,
    user_id : u64,
    ctx : Context
}

pub struct Handler {

    pub file_size_limit_bytes : i64,
    pub file_duration_max : Duration,

    sink : Arc<dyn chimes::ChimeSink>,

    watchers : Mutex<HashMap<u64, JoinHandle<()>>>,

    bus : Mutex<bus::Bus<BusChimePayload>>
}
impl Handler {
    pub fn new(
        sink: Arc<dyn chimes::ChimeSink>, 
        bus_size : usize, 
        file_duration_max : Duration
    ) -> Self {
        Self 
        {
            file_size_limit_bytes : -1,
            file_duration_max,
            sink: Arc::clone(&sink),
            watchers : Mutex::new(HashMap::new()),
            bus : Mutex::new(bus::Bus::new(bus_size))            
        }
    }

    async fn spawn_guild_watcher(
        &self, 
        guild_id : u64
    ) -> JoinHandle<()> {
        let mut task_rx = self.bus.lock().await.add_rx();
        let sink_arc = Arc::clone(&self.sink);
        task::spawn(async move {
            while let Ok(msg) = task_rx.recv() {

                if msg.guild_id != guild_id {                
                    continue;
                }

                let manager = songbird::get(&msg.ctx).await;
                if manager.is_none() {
                    error!("Could not get songbird!");
                    continue;
                }
                let manager = Arc::clone(&manager.unwrap());
                let (call, result) = manager.join(msg.guild_id, msg.channel_id).await;
                if let Err(why) = result {
                    error!("Could not join guild: {}", why);
                    continue
                }

                if let Ok(chime) = sink_arc.get_input(msg.user_id).await {

                    let player = call.lock().await.play_only_source(chime);

                    // block until track is finished
                    if let Some(duration) = player.metadata().duration {
                        tokio::time::sleep(duration).await;
                    }
                }

                // will disconnect after every playback FIXME
                let _ = manager.leave(guild_id).await.map_err(|err| error!("Could not leave guild {}", err));
            }

            info!("Ended task for guild {}", guild_id);
        })
    }

    async fn process_chime_data(
        &self, 
        data : &[u8], 
        user_id : u64, 
        filename : Option<&str>
    ) -> Result<(), AttachmentError> {

        let mut temp_path = temp_dir();
        match filename {
            Some(name) => temp_path.push(name),
            None => temp_path.push(uuid::Uuid::new_v4().to_string())
        };

        let temp_file = File::create(&temp_path);
        if temp_file.is_err() {
            error!("Could not create temporary file {:#?}", temp_file);
            return Err(AttachmentError::Tempfile);
        }
        let temp_file = temp_file.unwrap();
        if let Err(why) = temp_file.write_all_at(&data, 0) {
            error!("Could not write data to file: {:#?}", why);
            return Err(AttachmentError::Tempfile);
        }

        match ffprobe(&temp_path) {
            Ok(info) => {
                let duration = info.format.duration; // seconds
                if duration.is_none() { 
                    return Err(AttachmentError::Unreadable); 
                }

                let parsed = duration.unwrap().parse::<f64>();
                if parsed.is_err() {
                    return Err(AttachmentError::Unreadable);
                }

                let duration = Duration::from_secs_f64(parsed.unwrap());
                if duration > self.file_duration_max {
                    return Err(AttachmentError::Duration);
                }
            },
            Err(why) => {
                error!("FFProbe on data failed: {:#?}", why);
                return Err(AttachmentError::Unreadable);
            }
        };

        match self.sink.save_data(user_id, temp_path).await {
            Ok(_) => Ok(()),
            Err(why) => {
                error!("Could not save chime to sink: {:?}", why);
                Err(AttachmentError::Tempfile)
            }
        }
    }
}
#[async_trait]
impl EventHandler for Handler {
    async fn guild_create(
        &self, 
        _ctx: Context, 
        guild: serenity::model::guild::Guild, 
        _is_new: bool
    ) {

        let guild_id = guild.id.0;

        let mut watchers = self.watchers.lock().await;

        if watchers.contains_key(&guild_id) {
            return
        }

        watchers.insert(
            guild_id, 
            self.spawn_guild_watcher(guild_id).await
        ); 
    }

    async fn ready(
        &self, 
        ctx: Context, 
        ready: Ready
    ) {
        use serenity::model::{
            gateway::Activity,
            user::OnlineStatus,
            prelude::command::{Command, CommandOptionType},
        };

        ctx.set_presence(Some(Activity::listening("/chime")), OnlineStatus::Online).await;

        info!("{} is connected!", ready.user.name);

        Command::set_global_application_commands(&ctx.http, |create_app_commands| {
            create_app_commands.create_application_command(|cmd| {
                cmd.name("dab")
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
                            .kind(CommandOptionType::SubCommandGroup)
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

    async fn voice_state_update(
        &self, 
        ctx: Context, 
        old: Option<serenity::model::voice::VoiceState>, 
        new: serenity::model::voice::VoiceState
    ) {

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

        if let Some(prev) = old.as_ref().and_then(|state| state.channel_id) {
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

        if let Some(old_guild) = old.and_then(|v| v.guild_id) {
            if old_guild == guild_id {
                return
            }
        }

        self.bus.lock().await.broadcast(
           BusChimePayload {
                guild_id : guild_id.0, 
                channel_id : channel_id.0, 
                user_id : user.id.0,
                ctx
            }
        )
    }

    async fn interaction_create(
        &self, 
        ctx: Context, 
        interaction: Interaction
    ) {
        if let Interaction::ApplicationCommand(command) = interaction {

            async fn respond(
                command : &ApplicationCommandInteraction,
                ctx : Context,
                success : bool,
                info : Option<&str>
            ) {
                if let Err(why) = command.create_interaction_response(&ctx.http, |response| {
                    response.kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|data| {
                                data.content(
                                    match info {
                                        Some(text) => text,
                                        None => if success { "success!" } else { "that did not work..." }
                                    }
                                )
                            })
                }).await {
                    error!("Error responding to interaction: {:?}", why);
                    return;
                }
            }

            info!("Received command interaction: {:#?}", command);

            let name = command.data.name.as_str();
            if name != "dab" { // todo make this dynamic based on config
                warn!("Unknown command received! {:#?}", name);
                return;
            }

            let user = command.user.id.0;

            // chime clear
            // chime set url/attachment
            let base_option = command
                .data
                .options
                .get(0);

            if base_option.is_none() {
                error!("Bad base option in command!");
                return
            }

            let base_option : &CommandDataOption = base_option.unwrap();

            /* _BIG_ match */
            match base_option.name.as_str() {
                "clear" => self.sink.clear_data(user).await,
                "set" => {

                    if base_option.options.len() != 1 {
                        warn!("Malformed command received {:#?}", base_option);
                    }

                    match &base_option.options.get(0).unwrap().resolved {
                        Some(CommandDataOptionValue::Attachment(attachment)) => {
                            if self.file_size_limit_bytes != -1 && 
                               attachment.size as i64 > self.file_size_limit_bytes 
                            {
                                info!("User {} supplied large file.", command.user.name);
                                respond(&command, ctx, false, Some("Whoops, too big!")).await;
                                return;
                            }

                            let data = attachment.download().await;
                            if data.is_err() {
                                warn!("Download failed! {:#?}", data);
                                respond(&command, ctx, false, Some("Download failed!")).await;
                                return;
                            }
                            let data = data.unwrap();

                            if let Err(_why) = self.process_chime_data(
                                &data, 
                                command.user.id.0, 
                                Some(&attachment.filename)
                            ).await {
                                respond(&command, ctx, false, None).await;
                                return;
                            }

                            respond(&command, ctx, true, None).await;
                        }, // attachment
                        Some(CommandDataOptionValue::String(url_str)) => {

                            let huh_weird_link = || respond(&command, ctx.clone(), false, Some("Huh, weird link!"));
                            
                            let url = url::Url::parse(url_str);
                            if url.is_err() {
                                warn!("User {} supplied bad url: {}", command.user.name, url_str);
                                huh_weird_link().await;
                                return;
                            }
                            let url = url.unwrap();

                            let response = reqwest::get(url.clone()).await;
                            if response.is_err() {
                                error!("Could not request {} : {:?}", url_str, response);
                                huh_weird_link().await;
                                return;
                            }
                            let response = response.unwrap();
                            let size = response.content_length();
                            if size.is_none() {
                                warn!("Bad header, no information about content-length");
                                huh_weird_link().await;
                                return;
                            }                            
                            if self.file_size_limit_bytes != -1 && 
                                size.unwrap() as i64 > self.file_size_limit_bytes 
                            {
                                info!("User {} supplied large file.", command.user.name);
                                respond(&command, ctx, false, Some("Whoops, too big!")).await;
                                return;
                            }

                            let download = response.text().await;
                            if download.is_err() {
                                let err = download.err().unwrap();
                                error!("Could not download from {} : {}", url, err);
                                respond(&command, ctx, false, Some(format!("{}", err).as_str())).await;
                                return;
                            }
                            let data = download.unwrap();

                            if let Err(why) = self.process_chime_data(
                                &data.as_bytes(),
                                command.user.id.0,
                                None
                            ).await {
                                respond(&command, ctx, false, Some(format!("{}", why).as_str())).await;
                                return;
                            }

                            respond(&command, ctx, true, None).await;
                        },// url
                        _ => warn!("Malformed command received {:#?}", base_option)

                    }; // match attachment, url
                }, // "set"
                val => warn!("Unknown option received! {}", val)
            }; // match name
        }
    }
}