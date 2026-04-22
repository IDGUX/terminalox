use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{KeyInfo, Server};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub servers: Vec<Server>,
    #[serde(default)]
    pub keys: Vec<KeyInfo>,
}

impl Store {
    pub fn load(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(path)?;
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn find_server(&self, id: Uuid) -> Option<&Server> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn find_server_mut(&mut self, id: Uuid) -> Option<&mut Server> {
        self.servers.iter_mut().find(|s| s.id == id)
    }

    pub fn find_key(&self, id: Uuid) -> Option<&KeyInfo> {
        self.keys.iter().find(|k| k.id == id)
    }
}

