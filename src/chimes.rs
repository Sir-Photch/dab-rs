use async_trait::async_trait;
use fs_extra::file::CopyOptions;
use log::{error, warn};
use songbird::input::*;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum ChimeSinkError {
    DataNotAvailable,
    Playback,
    SaveError,
    DirError,
}

#[async_trait]
pub trait ChimeSink: Send + Sync {
    async fn has_data(&self, user_id: u64) -> bool;
    async fn get_input(&self, user_id: u64) -> Result<Input, ChimeSinkError>;
    async fn save_data(&self, user_id: u64, file: std::path::PathBuf)
        -> Result<(), ChimeSinkError>;
    async fn clear_data(&self, user_id: u64);
}

pub struct FileChimeSink {
    dir: std::path::PathBuf,
    chimes: Mutex<HashMap<u64, std::path::PathBuf>>,
}
impl FileChimeSink {
    pub async fn new(mut dir: std::path::PathBuf) -> Result<Self, ChimeSinkError> {
        if dir.is_file() {
            return Err(ChimeSinkError::DirError);
        }

        if !dir.is_dir() {
            if let Err(why) = std::fs::create_dir_all(&dir) {
                error!("Could not ensure directory at {} : {}", dir.display(), why);
                return Err(ChimeSinkError::DirError);
            }
        }

        if dir.is_relative() {
            match dir.canonicalize() {
                Ok(canonical) => dir = canonical,
                Err(why) => {
                    error!("Could not canonicalize directory! {:#?}", why);
                    return Err(ChimeSinkError::DirError);
                }
            }
        }

        let paths = std::fs::read_dir(&dir);
        if paths.is_err() {
            return Err(ChimeSinkError::DirError);
        }

        let chimes = Mutex::new(HashMap::<u64, std::path::PathBuf>::new());
        let mut chimes_locked = chimes.lock().await;

        for path in paths.unwrap() {
            if path.is_err() {
                warn!("Bad path: {:#?}", path);
                continue;
            }

            let path = path.unwrap().path();

            if !path.is_file() {
                continue;
            }

            if let Some(prefix) = path.file_stem() {
                let user_id = prefix.to_str().and_then(|s| s.parse::<u64>().ok());
                if user_id.is_none() {
                    warn!("Invalid file in directory: {:#?}", prefix);
                    continue;
                }

                chimes_locked.insert(user_id.unwrap(), path);
            }
        }

        if chimes_locked.len() == 0 {
            warn!("No chimes found");
        }

        drop(chimes_locked);

        Ok(Self { dir, chimes })
    }
}
#[async_trait]
impl ChimeSink for FileChimeSink {
    async fn has_data(&self, user_id: u64) -> bool {
        self.chimes.lock().await.contains_key(&user_id)
    }

    async fn get_input(&self, user_id: u64) -> Result<Input, ChimeSinkError> {
        match self.chimes.lock().await.get(&user_id) {
            Some(input) => match ffmpeg(input).await {
                Ok(inp) => Ok(inp),
                Err(why) => {
                    error!("Could not playback chime {:#?}", why);
                    Err(ChimeSinkError::Playback)
                }
            },
            None => Err(ChimeSinkError::DataNotAvailable),
        }
    }

    async fn save_data(
        &self,
        user_id: u64,
        file: std::path::PathBuf,
    ) -> Result<(), ChimeSinkError> {
        let mut new_path = self.dir.clone();
        new_path.push(format!("{user_id}"));

        if self.has_data(user_id).await {
            self.clear_data(user_id).await;
        }

        match fs_extra::file::move_file(&file, &new_path, &CopyOptions::default()) {
            Ok(_) => {
                #[cfg(unix)]
                {
                    use std::{fs, os::unix::fs::PermissionsExt};
                    if let Some(parent) = new_path.parent() {
                        match fs::metadata(parent) {
                            Ok(metadata) => {
                                // remove x flag while respecting parent dir
                                let mode = metadata.permissions().mode() & !0o111;
                                if let Err(why) =
                                    fs::set_permissions(&new_path, fs::Permissions::from_mode(mode))
                                {
                                    warn!(
                                        "Could not set permissions of file {} to {}: {}",
                                        new_path.display(),
                                        mode,
                                        why
                                    );
                                }
                            }
                            Err(why) => warn!(
                                "Could not retrieve metadata for path {}: {}",
                                parent.display(),
                                why
                            ),
                        }
                    } else {
                        warn!("File {} has no parent!", new_path.display())
                    }
                }
                self.chimes.lock().await.insert(user_id, new_path);
                Ok(())
            }
            Err(why) => {
                error!(
                    "Could not move file {} to {}: {}",
                    file.display(),
                    new_path.display(),
                    why
                );
                Err(ChimeSinkError::SaveError)
            }
        }
    }

    async fn clear_data(&self, user_id: u64) {
        if let Some(path) = self.chimes.lock().await.remove(&user_id) {
            if let Err(why) = std::fs::remove_file(path) {
                error!("Could not remove entry for user: {:#?}", why);
            }
        }
    }
}
