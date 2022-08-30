use async_trait::async_trait;
use async_std::sync::Mutex;
use std::collections::HashMap;
use songbird::input::*;
use log::{warn, error};

#[derive(Debug)]
pub enum ChimeSinkError {
    DataNotAvailable,
    Playback,
    SaveError,
    DirError
}

#[async_trait]
pub trait ChimeSink: Send + Sync {
    async fn has_data(&self, user_id: u64) -> bool; 
    async fn get_input(&self, user_id: u64) -> Result<Input, ChimeSinkError>;
    async fn save_data(&self, user_id: u64, data: &[u8]) -> Result<(), ChimeSinkError>;
    async fn clear_data(&self, user_id: u64);
}

pub struct FileChimeSink {
    dir : std::path::PathBuf,
    chimes : Mutex<HashMap<u64, std::path::PathBuf>>,
    volatile : bool
}
impl FileChimeSink {
    pub async fn new(dir : std::path::PathBuf, volatile : bool) -> Result<Self, ChimeSinkError> {
        if !dir.is_dir() {
            return Err(ChimeSinkError::DirError);
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
                let user_id = prefix.to_str()
                                               .and_then(|s| s.parse::<u64>().ok() );
                if user_id == None {
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

        Ok(Self { dir, chimes, volatile })
    }
}
impl Drop for FileChimeSink {
    fn drop(&mut self) {
        if !self.volatile { return }

        if let Some(map) = self.chimes.try_lock() {
            for (_, v) in map.iter() {
                if let Err(why) = std::fs::remove_file(v) {
                    error!("Could not remove file {:#?}", why);
                }
            }
        } else {
            error!("Could not acquire lock while dropping FileChimeSink");
        }
    }
}
#[async_trait]
impl ChimeSink for FileChimeSink {    

    async fn has_data(&self, user_id: u64) -> bool {
        self.chimes.lock().await.contains_key(&user_id)
    }

    async fn get_input(&self, user_id: u64) -> Result<Input, ChimeSinkError> {
        match self.chimes.lock().await.get(&user_id) {
            Some(input) => {
                match ffmpeg(input).await {
                    Ok(inp) => Ok(inp),
                    Err(why) => {
                        error!("Could not playback chime {:#?}", why);
                        Err(ChimeSinkError::Playback)
                    }
                }
            }
            None => Err(ChimeSinkError::DataNotAvailable)
        }
    }

    async fn save_data(&self, user_id: u64, data: &[u8]) -> Result<(), ChimeSinkError> {
        todo!()
    }

    async fn clear_data(&self, user_id: u64) {
        if let Some(path) = self.chimes.lock().await.get(&user_id) {
            if let Err(why) = std::fs::remove_file(path) {
                error!("Could not remove entry for user: {:#?}", why);
            }
        }
    }
}