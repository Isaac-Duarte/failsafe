use std::path::Path;

pub fn notify_files_received(sender_name: &str, file_count: usize, destination: &Path) {
    let summary = if file_count == 1 {
        format!("Received 1 file from {sender_name}")
    } else {
        format!("Received {file_count} files from {sender_name}")
    };

    if let Err(error) = notify_rust::Notification::new()
        .summary("Failsafe")
        .body(&format!("{summary}\nSaved to {}", destination.display()))
        .show()
    {
        tracing::warn!("failed to show receive notification: {error}");
    }
}
