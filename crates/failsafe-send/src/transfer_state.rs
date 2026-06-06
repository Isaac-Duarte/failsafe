use std::path::{Path, PathBuf};

use failsafe_core::device::DeviceId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::payload::FileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendStage {
    Importing,
    ReadyToSend,
    WaitingAck,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveStage {
    Downloading,
    Exporting,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendTransferState {
    pub transfer_id: Uuid,
    pub target: DeviceId,
    pub paths: Vec<PathBuf>,
    pub stage: SendStage,
    pub collection_hash: Option<String>,
    pub entries: Vec<FileEntry>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiveTransferState {
    pub transfer_id: Uuid,
    pub sender: DeviceId,
    pub sender_name: String,
    pub stage: ReceiveStage,
    pub collection_hash: String,
    pub entries: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_dir: Option<PathBuf>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn transfers_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("failsafe").join("transfers"))
}

fn send_dir() -> Option<PathBuf> {
    transfers_dir().map(|dir| dir.join("send"))
}

fn receive_dir() -> Option<PathBuf> {
    transfers_dir().map(|dir| dir.join("receive"))
}

fn send_path(transfer_id: Uuid) -> Option<PathBuf> {
    send_dir().map(|dir| dir.join(format!("{transfer_id}.json")))
}

fn receive_path(transfer_id: Uuid) -> Option<PathBuf> {
    receive_dir().map(|dir| dir.join(format!("{transfer_id}.json")))
}

async fn write_state<T: Serialize>(path: &Path, state: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("failed to create transfer state dir: {error}"))?;
    }
    let payload = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("failed to encode transfer state: {error}"))?;
    let temp_path = path.with_extension("json.tmp");
    tokio::fs::write(&temp_path, &payload)
        .await
        .map_err(|error| format!("failed to write transfer state: {error}"))?;
    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(|error| format!("failed to commit transfer state: {error}"))
}

async fn read_state<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let payload = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to read transfer state: {error}"))?;
    serde_json::from_slice(&payload)
        .map_err(|error| format!("failed to decode transfer state: {error}"))
}

pub async fn save_send_state(state: &SendTransferState) -> Result<(), String> {
    let path = send_path(state.transfer_id)
        .ok_or_else(|| "could not determine transfer state directory".to_owned())?;
    write_state(&path, state).await
}

pub async fn load_send_state(transfer_id: Uuid) -> Result<SendTransferState, String> {
    let path = send_path(transfer_id)
        .ok_or_else(|| "could not determine transfer state directory".to_owned())?;
    if !path.exists() {
        return Err(format!("no send transfer state found for {transfer_id}"));
    }
    read_state(&path).await
}

pub async fn remove_send_state(transfer_id: Uuid) -> Result<(), String> {
    let Some(path) = send_path(transfer_id) else {
        return Ok(());
    };
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| format!("failed to remove send transfer state: {error}"))?;
    }
    Ok(())
}

pub async fn list_incomplete_sends() -> Result<Vec<SendTransferState>, String> {
    let Some(dir) = send_dir() else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|error| format!("failed to read send transfer dir: {error}"))?;
    let mut states = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("failed to read send transfer entry: {error}"))?
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let state: SendTransferState = read_state(&path).await?;
        if !matches!(state.stage, SendStage::Complete) {
            states.push(state);
        }
    }
    Ok(states)
}

pub async fn save_receive_state(state: &ReceiveTransferState) -> Result<(), String> {
    let path = receive_path(state.transfer_id)
        .ok_or_else(|| "could not determine transfer state directory".to_owned())?;
    write_state(&path, state).await
}

pub async fn load_receive_state(transfer_id: Uuid) -> Result<ReceiveTransferState, String> {
    let path = receive_path(transfer_id)
        .ok_or_else(|| "could not determine transfer state directory".to_owned())?;
    if !path.exists() {
        return Err(format!("no receive transfer state found for {transfer_id}"));
    }
    read_state(&path).await
}

pub async fn remove_receive_state(transfer_id: Uuid) -> Result<(), String> {
    let Some(path) = receive_path(transfer_id) else {
        return Ok(());
    };
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| format!("failed to remove receive transfer state: {error}"))?;
    }
    Ok(())
}

pub async fn list_incomplete_receives() -> Result<Vec<ReceiveTransferState>, String> {
    let Some(dir) = receive_dir() else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|error| format!("failed to read receive transfer dir: {error}"))?;
    let mut states = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("failed to read receive transfer entry: {error}"))?
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let state: ReceiveTransferState = read_state(&path).await?;
        if !matches!(state.stage, ReceiveStage::Complete) {
            states.push(state);
        }
    }
    Ok(states)
}
