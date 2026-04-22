use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

use crate::storage::Store;

pub struct SessionHandle {
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub resize_tx: mpsc::UnboundedSender<(u32, u32)>,
    pub cancel: Option<oneshot::Sender<()>>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub store: Arc<RwLock<Store>>,
    pub sessions: Arc<RwLock<HashMap<Uuid, SessionHandle>>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, store: Store) -> Self {
        Self {
            data_dir,
            store: Arc::new(RwLock::new(store)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn store_file(&self) -> PathBuf {
        self.data_dir.join("store.json")
    }
}
