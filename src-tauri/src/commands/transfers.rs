use super::{blocking, CmdResult, Shared};
use crate::plan::WorkspaceContext;
use crate::transfer::{ConflictDecision, TransferJob};
use crate::wipe::{WipeMethod, WipePlan};
use tauri::State;

#[tauri::command]
pub async fn run_flow(
    state: State<'_, Shared>,
    project_id: String,
    flow_id: String,
) -> CmdResult<String> {
    blocking(&state, move |s| s.core.run_flow(&project_id, &flow_id)).await
}

#[tauri::command]
pub async fn run_all(state: State<'_, Shared>, project_id: String) -> CmdResult<Vec<String>> {
    blocking(&state, move |s| s.core.run_all(&project_id)).await
}

#[tauri::command]
pub async fn run_workspace_flow(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    flow_id: String,
) -> CmdResult<String> {
    blocking(&state, move |s| {
        s.core.run_workspace_flow(&context, &flow_id)
    })
    .await
}

#[tauri::command]
pub async fn run_workspace_all(
    state: State<'_, Shared>,
    context: WorkspaceContext,
) -> CmdResult<Vec<String>> {
    blocking(&state, move |s| s.core.run_workspace_all(&context)).await
}

#[tauri::command]
pub async fn check_workspace_destination(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    destination_id: String,
) -> CmdResult<Vec<String>> {
    blocking(&state, move |s| {
        s.core
            .check_workspace_destination(&context, &destination_id)
    })
    .await
}

#[tauri::command]
pub async fn run_workspace_destination(
    state: State<'_, Shared>,
    context: WorkspaceContext,
    destination_id: String,
) -> CmdResult<Vec<String>> {
    blocking(&state, move |s| {
        s.core.run_workspace_destination(&context, &destination_id)
    })
    .await
}

#[tauri::command]
pub fn resolve_transfer_conflict(
    state: State<'_, Shared>,
    job_id: String,
    request_id: String,
    decision: ConflictDecision,
    apply_to_remaining: bool,
) -> CmdResult<()> {
    state
        .core
        .transfers
        .resolve_conflict(&job_id, &request_id, decision, apply_to_remaining)
}

#[tauri::command]
pub fn set_transfer_paused(state: State<'_, Shared>, job_id: String, paused: bool) {
    state.core.transfers.set_paused(&job_id, paused);
}

#[tauri::command]
pub fn set_all_paused(state: State<'_, Shared>, paused: bool) {
    state.core.transfers.set_all_paused(paused);
}

#[tauri::command]
pub fn cancel_transfer(state: State<'_, Shared>, job_id: String) {
    state.core.transfers.cancel(&job_id);
}

#[tauri::command]
pub fn list_transfers(state: State<'_, Shared>) -> Vec<TransferJob> {
    state.core.transfers.jobs()
}

#[tauri::command]
pub async fn plan_wipe(
    state: State<'_, Shared>,
    project_id: String,
    source_id: String,
) -> CmdResult<WipePlan> {
    blocking(&state, move |s| {
        crate::wipe::plan_wipe(
            &s.core.store,
            s.core.resolver.as_ref(),
            &project_id,
            &source_id,
        )
    })
    .await
}

#[tauri::command]
pub async fn wipe(
    state: State<'_, Shared>,
    project_id: String,
    source_id: String,
    method: WipeMethod,
) -> CmdResult<String> {
    blocking(&state, move |s| {
        s.core.start_wipe(&project_id, &source_id, method)
    })
    .await
}
