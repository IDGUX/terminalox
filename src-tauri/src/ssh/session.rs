use base64::prelude::*;
use russh::ChannelMsg;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{AuthMode, KeyInfo, Server};
use crate::ssh::client;
use crate::state::SessionHandle;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionDataEvent {
    pub session_id: Uuid,
    /// Raw PTY bytes, base64-encoded for efficient JSON transport.
    pub data_b64: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionClosedEvent {
    pub session_id: Uuid,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
}

pub struct SessionLaunchParams {
    pub server: Server,
    pub key: Option<KeyInfo>,
    pub password: Option<String>,
    pub cols: u32,
    pub rows: u32,
}

pub async fn spawn_session(
    app: AppHandle,
    params: SessionLaunchParams,
) -> AppResult<(Uuid, SessionHandle)> {
    let session_id = Uuid::new_v4();

    let mut ssh = client::connect(&params.server.host, params.server.port).await?;
    match &params.server.auth {
        AuthMode::Password => {
            let password = params
                .password
                .as_deref()
                .ok_or_else(|| AppError::Other("password required".into()))?;
            client::auth_password(&mut ssh, &params.server.user, password).await?;
        }
        AuthMode::Key { .. } => {
            let key = params
                .key
                .ok_or_else(|| AppError::Other("key not found for server".into()))?;
            client::auth_key(&mut ssh, &params.server.user, &key.private_path).await?;
        }
    }

    let mut channel = ssh.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", params.cols, params.rows, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    let app_for_task = app.clone();
    tokio::spawn(async move {
        let mut final_error: Option<String> = None;
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
                        final_error = Some(format!("write: {e}"));
                        break;
                    }
                }
                Some((cols, rows)) = resize_rx.recv() => {
                    let _ = channel.window_change(cols, rows, 0, 0).await;
                }
                msg = channel.wait() => {
                    let Some(msg) = msg else { break; };
                    match msg {
                        ChannelMsg::Data { ref data } => {
                            let encoded = BASE64_STANDARD.encode(&data[..]);
                            let _ = app_for_task.emit(
                                &event_name(session_id, "data"),
                                SessionDataEvent {
                                    session_id,
                                    data_b64: encoded,
                                },
                            );
                        }
                        ChannelMsg::ExtendedData { ref data, ext: _ } => {
                            let encoded = BASE64_STANDARD.encode(&data[..]);
                            let _ = app_for_task.emit(
                                &event_name(session_id, "data"),
                                SessionDataEvent {
                                    session_id,
                                    data_b64: encoded,
                                },
                            );
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

        let _ = ssh
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
        let _ = app_for_task.emit(
            &event_name(session_id, "closed"),
            SessionClosedEvent {
                session_id,
                exit_code,
                error: final_error,
            },
        );
    });

    Ok((
        session_id,
        SessionHandle {
            input_tx,
            resize_tx,
            cancel: Some(cancel_tx),
        },
    ))
}

pub fn event_name(session_id: Uuid, suffix: &str) -> String {
    format!("session:{session_id}:{suffix}")
}
