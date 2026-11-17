use std::collections::HashMap;
use std::sync::Arc;

use russh::ChannelMsg;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

use crate::access::Login;
use crate::error::AppResult;
use crate::msg::Msg;
use crate::state::SessionHandle;

/// Pending keystrokes/pastes per session; `write_session` waits when full.
const INPUT_QUEUE: usize = 256;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionClosedEvent {
    pub session_id: Uuid,
    pub exit_code: Option<u32>,
    pub error: Option<Msg>,
}

/// Frontend channels: raw PTY bytes (ArrayBuffer on the JS side) and one
/// closing message.
pub struct SessionOutput {
    pub data: Channel<InvokeResponseBody>,
    pub closed: Channel<SessionClosedEvent>,
}

pub struct SessionLaunchParams {
    pub login: Login,
    pub cols: u32,
    pub rows: u32,
}

pub async fn spawn_session(
    params: SessionLaunchParams,
    output: SessionOutput,
    sessions: Arc<RwLock<HashMap<Uuid, SessionHandle>>>,
) -> AppResult<Uuid> {
    let session_id = Uuid::new_v4();

    let ssh = params.login.connect().await?;

    let mut channel = ssh.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", params.cols, params.rows, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;

    let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(INPUT_QUEUE);
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    // Register before the task starts, so a session that ends immediately
    // still finds (and removes) its own entry.
    sessions.write().await.insert(
        session_id,
        SessionHandle {
            input_tx,
            resize_tx,
            cancel: Some(cancel_tx),
        },
    );

    tokio::spawn(async move {
        let mut final_error: Option<Msg> = None;
        let mut exit_code: Option<u32> = None;

        loop {
            tokio::select! {
                biased;
                _ = &mut cancel_rx => {
                    let _ = channel.eof().await;
                    break;
                }
                Some(bytes) = input_rx.recv() => {
                    if let Err(e) = channel.data(bytes.as_slice()).await {
                        final_error = Some(msg!("send_failed", "Sending failed: {detail}", detail = e));
                        break;
                    }
                }
                Some((cols, rows)) = resize_rx.recv() => {
                    let _ = channel.window_change(cols, rows, 0, 0).await;
                }
                msg = channel.wait() => {
                    let Some(msg) = msg else { break; };
                    match msg {
                        ChannelMsg::Data { ref data } | ChannelMsg::ExtendedData { ref data, .. } => {
                            if output.data.send(InvokeResponseBody::Raw(data.to_vec())).is_err() {
                                // Frontend is gone (window closed / reloaded).
                                break;
                            }
                        }
                        ChannelMsg::ExitStatus { exit_status } => {
                            exit_code = Some(exit_status);
                        }
                        ChannelMsg::Eof | ChannelMsg::Close => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        sessions.write().await.remove(&session_id);
        let _ = ssh
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
        let _ = output.closed.send(SessionClosedEvent {
            session_id,
            exit_code,
            error: final_error,
        });
    });

    Ok(session_id)
}
