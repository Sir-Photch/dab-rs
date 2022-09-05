use crate::*;

use ffprobe::ffprobe;
use log::{error, info, warn};
use serenity::{
    async_trait,
    builder::{CreateApplicationCommand, CreateApplicationCommandOption},
    model::{
        application::interaction::{
            application_command::{
                ApplicationCommandInteraction, CommandDataOption, CommandDataOptionValue,
            },
            Interaction, InteractionResponseType,
        },
        gateway::Ready,
    },
    prelude::*,
};
use std::{
    env::temp_dir, fmt::Display, fs::File, os::unix::prelude::FileExt, sync::Arc, time::Duration,
};
use tokio::{
    sync::{Mutex, MutexGuard},
    task::{self, JoinHandle},
};

#[derive(Debug)]
enum AttachmentError {
    Duration,
    Unreadable,
    Tempfile,
}
impl Display for AttachmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // use localization strings here
            AttachmentError::Duration => write!(f, "duration-exceeded"),
            AttachmentError::Unreadable => write!(f, "data-unreadable"),
            AttachmentError::Tempfile => write!(f, "internal-error"),
        }
    }
}

#[derive(Clone)]
struct BusChimePayload {
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    ctx: Context,
}

pub struct Handler {
    file_size_limit_bytes: i64,
    file_duration_max: Duration,
    command_root: String,
    disconnect_timeout: Duration,

    sink: Arc<dyn chimes::ChimeSink>,
    watchers: Mutex<HashMap<u64, JoinHandle<()>>>,
    cleanup_watcher: Mutex<Option<JoinHandle<()>>>,
    flag_map: Arc<Mutex<HashMap<u64, bool>>>,
    bus: Mutex<bus::Bus<BusChimePayload>>,

    latest_context: Arc<Mutex<Option<Context>>>,

    localizer: Mutex<fluent::FluentLocalizer>,
}
impl Handler {
    // is there a better way?
    pub fn new(
        sink: Arc<dyn chimes::ChimeSink>,
        bus_size: usize,
        file_duration_max: Duration,
        file_size_limit_bytes: i64,
        command_root: String,
        disconnect_timeout: Duration,
        localizer: Mutex<fluent::FluentLocalizer>,
    ) -> Self {
        Self {
            file_size_limit_bytes,
            file_duration_max,
            command_root,
            disconnect_timeout,
            sink: Arc::clone(&sink),
            watchers: Mutex::new(HashMap::new()),
            bus: Mutex::new(bus::Bus::new(bus_size)),
            flag_map: Arc::new(Mutex::new(HashMap::new())),
            cleanup_watcher: Mutex::new(None),
            latest_context: Arc::new(Mutex::new(None)),
            localizer,
        }
    }

    async fn spawn_cleanup_watcher(&self) -> JoinHandle<()> {
        let timeout = self.disconnect_timeout;
        let flags = Arc::clone(&self.flag_map);
        let ctx = Arc::clone(&self.latest_context);

        task::spawn(async move {
            let mut context = Option::<Context>::None;

            loop {
                tokio::time::sleep(timeout).await;

                if let Some(ctx_buf) = ctx.lock().await.take() {
                    context = Some(ctx_buf);
                }
                if context.is_none() {
                    continue;
                }

                let bird = songbird::get(&context.clone().unwrap()).await;
                if bird.is_none() {
                    error!("Could not get songbird, invalid context!");
                    continue;
                }
                let bird = Arc::clone(&bird.unwrap());

                let mut flag_lock = flags.lock().await;

                for (key, v) in flag_lock.iter_mut() {
                    if *v {
                        *v = false;
                    } else if bird.get(*key).is_some() {
                        if let Err(why) = bird.leave(*key).await {
                            error!("Could not cleanup guild: {}", why);
                        }
                    }
                }
            }
        })
    }

    async fn spawn_guild_watcher(&self, guild_id: u64) -> JoinHandle<()> {
        let mut task_rx = self.bus.lock().await.add_rx();
        let sink_arc = Arc::clone(&self.sink);
        let flags = Arc::clone(&self.flag_map);

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
                    continue;
                }

                flags.lock().await.insert(guild_id, true);

                if let Ok(chime) = sink_arc.get_input(msg.user_id).await {
                    // dont keep mutex-guards for too long
                    if let Err(why) = call.lock().await.deafen(true).await {
                        error!("Could not deafen: {:?}", why);
                    }
                    let player = call.lock().await.play_only_source(chime);

                    if let Some(duration) = player.metadata().duration {
                        tokio::time::sleep(duration).await;
                    } else {
                        warn!("Track has no duration!");
                    }
                }
            }

            info!("Ended task for guild {}", guild_id);
        })
    }

    async fn process_chime_data(
        &self,
        data: &[u8],
        user_id: u64,
        filename: Option<&str>,
    ) -> Result<(), AttachmentError> {
        let mut temp_path = temp_dir();
        match filename {
            Some(name) => temp_path.push(name),
            None => temp_path.push(uuid::Uuid::new_v4().to_string()),
        };

        let temp_file = File::create(&temp_path);
        if temp_file.is_err() {
            error!("Could not create temporary file {:?}", temp_file);
            return Err(AttachmentError::Tempfile);
        }
        let temp_file = temp_file.unwrap();
        if let Err(why) = temp_file.write_all_at(data, 0) {
            error!("Could not write data to file: {:?}", why);
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
            }
            Err(why) => {
                error!("FFProbe on data failed: {:?}", why);
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

    async fn respond(
        &self,
        command: &ApplicationCommandInteraction,
        ctx: Context,
        success: bool,
        info: Option<&str>,
    ) {
        let msg = match info {
            Some(text) => text,
            None => {
                if success {
                    "success"
                } else {
                    "fail"
                }
            }
        };
        let msg = self
            .localizer
            .lock()
            .await
            .localize(&command.locale, msg, None)
            .into_owned();

        if let Err(why) = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|data| data.content(msg).ephemeral(true))
            })
            .await
        {
            error!("Error responding to interaction: {:?}", why);
        }
    }

    fn localize_command<'a>(
        guard: &MutexGuard<fluent::FluentLocalizer>,
        available_locales: &[String],
        cmd: &'a mut CreateApplicationCommand,
        msg: &str,
    ) -> &'a mut CreateApplicationCommand {
        let default_locale = guard.fallback_locale.to_string();
        let mut cmd = cmd.description(guard.localize(&default_locale, msg, None));

        for loc in available_locales.iter().filter(|s| **s != default_locale) {
            cmd = cmd.description_localized(loc.as_str(), guard.localize(loc, msg, None));
        }
        cmd
    }

    fn localize_option<'a>(
        guard: &MutexGuard<fluent::FluentLocalizer>,
        available_locales: &[String],
        opt: &'a mut CreateApplicationCommandOption,
        msg: &str,
    ) -> &'a mut CreateApplicationCommandOption {
        let default_locale = guard.fallback_locale.to_string();
        let mut opt = opt.description(guard.localize(&default_locale, msg, None));

        for loc in available_locales.iter().filter(|s| **s != default_locale) {
            opt = opt.description_localized(loc.as_str(), guard.localize(loc, msg, None));
        }
        opt
    }
}
#[async_trait]
impl EventHandler for Handler {
    async fn guild_create(
        &self,
        ctx: Context,
        guild: serenity::model::guild::Guild,
        _is_new: bool,
    ) {
        let guild_id = guild.id.0;

        let mut watchers = self.watchers.lock().await;

        if watchers.contains_key(&guild_id) {
            return;
        }

        watchers.insert(guild_id, self.spawn_guild_watcher(guild_id).await);

        let _ = self.latest_context.lock().await.insert(ctx);
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        use serenity::model::{
            gateway::Activity,
            prelude::command::{Command, CommandOptionType},
            user::OnlineStatus,
        };

        ctx.set_presence(
            Some(Activity::listening(format!("/{}", self.command_root))),
            OnlineStatus::Online,
        )
        .await;

        info!("{} is connected!", ready.user.name);

        let localizer_lock = self.localizer.lock().await;
        let available_locales = localizer_lock.get_available_localizations();

        Command::set_global_application_commands(&ctx.http, |create_app_commands| {
            create_app_commands.create_application_command(|cmd| {
                Self::localize_command(&localizer_lock, &available_locales, cmd, "base")
                    .name(self.command_root.as_str()) // 0
                    .create_option(|opt| {
                        Self::localize_option(
                            &localizer_lock,
                            &available_locales,
                            opt,
                            "base-clear",
                        )
                        .name("clear") // 0
                        .kind(CommandOptionType::SubCommand)
                    })
                    .create_option(|opt| {
                        Self::localize_option(&localizer_lock, &available_locales, opt, "base-set")
                            .name("set") // 1
                            .kind(CommandOptionType::SubCommandGroup)
                            .create_sub_option(|opt| {
                                Self::localize_option(
                                    &localizer_lock,
                                    &available_locales,
                                    opt,
                                    "base-set-file",
                                )
                                .name("file") // 0
                                .kind(CommandOptionType::SubCommand)
                                .create_sub_option(|opt| {
                                    Self::localize_option(
                                        &localizer_lock,
                                        &available_locales,
                                        opt,
                                        "base-set-file-attachment",
                                    )
                                    .name("attachment") // 0
                                    .kind(CommandOptionType::Attachment)
                                    .required(true)
                                })
                            })
                            .create_sub_option(|opt| {
                                Self::localize_option(
                                    &localizer_lock,
                                    &available_locales,
                                    opt,
                                    "base-set-url",
                                )
                                .name("url") // 1
                                .kind(CommandOptionType::SubCommand)
                                .create_sub_option(|opt| {
                                    Self::localize_option(
                                        &localizer_lock,
                                        &available_locales,
                                        opt,
                                        "base-set-url-link",
                                    )
                                    .name("link") // 0
                                    .kind(CommandOptionType::String)
                                    .required(true)
                                })
                            })
                    })
            })
        })
        .await
        .expect("could not set commands!");

        let _ = self.latest_context.lock().await.insert(ctx);
        let _ = self
            .cleanup_watcher
            .lock()
            .await
            .insert(self.spawn_cleanup_watcher().await);
    }

    async fn voice_state_update(
        &self,
        ctx: Context,
        old: Option<serenity::model::voice::VoiceState>,
        new: serenity::model::voice::VoiceState,
    ) {
        let user = new.user_id.to_user(&ctx.http).await;
        if user.is_err() {
            error!("Unexpected error, user could not be retrieved! {:?}", user);
            return;
        }
        let user = user.unwrap();

        if user.bot {
            return;
        }

        if new.channel_id == None {
            return;
        }
        let channel_id = new.channel_id.unwrap();

        if let Some(prev) = old.as_ref().and_then(|state| state.channel_id) {
            if prev == channel_id {
                return;
            }
        }

        if !self.sink.has_data(user.id.0).await {
            return;
        }

        if new.guild_id == None {
            warn!("Unexpected: user connected to unknown guild");
            return;
        }
        let guild_id = new.guild_id.unwrap();

        if let Some(old_guild) = old.and_then(|v| v.guild_id) {
            if old_guild == guild_id {
                return;
            }
        }

        let _ = self.latest_context.lock().await.insert(ctx.clone());

        self.bus.lock().await.broadcast(BusChimePayload {
            guild_id: guild_id.0,
            channel_id: channel_id.0,
            user_id: user.id.0,
            ctx,
        });
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            info!("Received command interaction: {:?}", command);

            let name = command.data.name.as_str();
            if name != self.command_root {
                warn!("Unknown command received! {:?}", name);
                return;
            }

            let user = command.user.id.0;

            // chime clear
            // chime set url/attachment
            let base_option = command.data.options.get(0);

            if base_option.is_none() {
                error!("Bad base option in command!");
                return;
            }

            let base_option: &CommandDataOption = base_option.unwrap();

            let username = command.user.tag();

            /* _BIG_ match */
            match base_option.name.as_str() {
                "clear" => {
                    self.sink.clear_data(user).await;
                    info!("User {username} cleared his chime");
                    self.respond(&command, ctx.clone(), true, None).await;
                }
                "set" => {
                    if base_option.options.len() != 1
                        || base_option.options.get(0).unwrap().options.len() != 1
                    {
                        warn!("Malformed command received {:?}", base_option);
                        return;
                    }

                    match &base_option
                        .options
                        .get(0)
                        .unwrap()
                        .options
                        .get(0)
                        .unwrap()
                        .resolved
                    {
                        Some(CommandDataOptionValue::Attachment(attachment)) => {
                            if self.file_size_limit_bytes != -1
                                && attachment.size as i64 > self.file_size_limit_bytes
                            {
                                info!("User {username} supplied large file");
                                self.respond(&command, ctx, false, Some("file-too-large"))
                                    .await;
                                return;
                            }

                            let data = attachment.download().await;
                            if data.is_err() {
                                warn!("Download failed! {:?}", data);
                                self.respond(&command, ctx, false, Some("download-failed"))
                                    .await;
                                return;
                            }
                            let data = data.unwrap();

                            if let Err(why) = self
                                .process_chime_data(
                                    &data,
                                    command.user.id.0,
                                    Some(&attachment.filename),
                                )
                                .await
                            {
                                info!("Checking chime data for user {username} failed: {:?}", why);
                                self.respond(&command, ctx, false, None).await;
                                return;
                            }

                            info!("User {username} changed his chime successfully.");
                            self.respond(&command, ctx.clone(), true, None).await;
                        } // attachment
                        Some(CommandDataOptionValue::String(url_str)) => {
                            let url = url::Url::parse(url_str);
                            if url.is_err() {
                                info!("User {username} supplied bad url: {url_str}");
                                self.respond(&command, ctx.clone(), false, Some("bad-url"))
                                    .await;
                                return;
                            }
                            let url = url.unwrap();

                            let response = reqwest::get(url).await;
                            if response.is_err() {
                                error!(
                                    "Could not request {url_str} for user {username}: {:?}",
                                    response
                                );
                                self.respond(&command, ctx.clone(), false, Some("bad-url"))
                                    .await;
                                return;
                            }
                            let response = response.unwrap();
                            let size = response.content_length();
                            if size.is_none() {
                                warn!("Bad header for url from user {username}, no information about content-length");
                                self.respond(&command, ctx.clone(), false, Some("bad-url"))
                                    .await;
                                return;
                            }
                            if self.file_size_limit_bytes != -1
                                && size.unwrap() as i64 > self.file_size_limit_bytes
                            {
                                info!("User {username} supplied large file.");
                                self.respond(&command, ctx, false, Some("file-too-large"))
                                    .await;
                                return;
                            }

                            let download = response.bytes().await;
                            if download.is_err() {
                                let err = download.err().unwrap();
                                error!(
                                    "Could not download for user {username} from {url_str} : {:?}",
                                    err
                                );
                                self.respond(
                                    &command,
                                    ctx,
                                    false,
                                    Some(format!("{}", err).as_str()),
                                )
                                .await;
                                return;
                            }
                            let data = download.unwrap();

                            if let Err(why) = self
                                .process_chime_data(&data, command.user.id.0, None)
                                .await
                            {
                                info!("Checking chime data for user {username} failed: {:?}", why);
                                self.respond(
                                    &command,
                                    ctx,
                                    false,
                                    Some(format!("{}", why).as_str()),
                                )
                                .await;
                                return;
                            }

                            info!("User {username} changed his chime successfully.");
                            self.respond(&command, ctx.clone(), true, None).await;
                        } // url
                        _ => warn!("Malformed command received {:?}", base_option),
                    }; // match attachment, url
                } // "set"
                val => warn!("Unknown option received! {}", val),
            }; // match name
        } // if let interaction

        let _ = self.latest_context.lock().await.insert(ctx);
    }
}
