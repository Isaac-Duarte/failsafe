use crate::coordinator::SendCoordinator;
use crate::transfer_state::{
    list_incomplete_receives, list_incomplete_sends, remove_receive_state, remove_send_state,
};

pub async fn cancel_all_incomplete_sends(
    coordinator: &SendCoordinator,
) -> Result<usize, String> {
    let states = list_incomplete_sends().await?;
    let count = states.len();
    for state in states {
        coordinator.cancel(state.transfer_id).await;
        remove_send_state(state.transfer_id).await?;
    }
    coordinator.cancel_all().await;
    Ok(count)
}

pub async fn cancel_all_incomplete_receives() -> Result<usize, String> {
    let states = list_incomplete_receives().await?;
    let count = states.len();
    for state in states {
        remove_receive_state(state.transfer_id).await?;
    }
    Ok(count)
}
