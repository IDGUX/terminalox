use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{Folder, KeyInfo, Server};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub servers: Vec<Server>,
    #[serde(default)]
    pub keys: Vec<KeyInfo>,
    #[serde(default)]
    pub folders: Vec<Folder>,
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

    pub fn find_folder(&self, id: Uuid) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Server, deren Ordner nicht (mehr) existiert, landen wieder in "Ohne Ordner".
    pub fn clear_dangling_folder_refs(&mut self) {
        let ids: Vec<Uuid> = self.folders.iter().map(|f| f.id).collect();
        for s in &mut self.servers {
            if matches!(s.folder_id, Some(id) if !ids.contains(&id)) {
                s.folder_id = None;
            }
        }
    }

    pub fn find_key(&self, id: Uuid) -> Option<&KeyInfo> {
        self.keys.iter().find(|k| k.id == id)
    }
}

