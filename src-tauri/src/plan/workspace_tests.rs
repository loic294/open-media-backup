use super::*;
use crate::domain::{EntityKind, Project};
use crate::testing::Fixture;

fn context(fx: &Fixture) -> WorkspaceContext {
    WorkspaceContext {
        space_id: fx.space.id.clone(),
        project_id: None,
    }
}

fn without_projects() -> Fixture {
    let fx = Fixture::new();
    fx.store
        .delete(EntityKind::Project, &fx.project.id)
        .unwrap();
    fx
}

fn status(fx: &Fixture) -> WorkspaceStatus {
    workspace_status(
        &fx.store,
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &context(fx),
        &FailureMap::new(),
    )
    .unwrap()
}

#[test]
fn project_free_status_has_real_counts_without_wipe_policy() {
    let fx = without_projects();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let st = status(&fx);
    assert_eq!(st.context, context(&fx));
    assert_eq!(st.sources.len(), 1);
    assert!(st.sources[0].available);
    assert_eq!(st.sources[0].file_count, 1);
    assert_eq!(st.sources[0].total_bytes, 5);
    assert_eq!(st.sources[0].required_copies, None);
    assert!(!st.sources[0].wipe_eligible);
    assert!(st.sources[0]
        .blocking_reason
        .as_deref()
        .unwrap()
        .contains("project"));
    assert_eq!(st.flows[0].state, FlowState::Pending);
    assert_eq!(st.destinations[0].to_transfer, 1);
    let ctx = resolve_workspace_flow(&fx.store, &fx.resolver, &context(&fx), "flow").unwrap();
    let planned = classify_flow(&ctx, &Catalog::load(&fx.store).unwrap(), None);
    assert_eq!(planned[0].project_id, None);
    assert_eq!(planned[0].category, Category::ToTransfer);
    assert!(fx.store.list::<Project>().unwrap().is_empty());
}

#[test]
fn project_free_offline_cards_still_have_terminal_status() {
    let fx = without_projects();
    fx.unmount("card");
    fx.unmount("nas");
    let st = status(&fx);
    assert_eq!(st.sources.len(), 1);
    assert_eq!(st.destinations.len(), 1);
    assert!(!st.sources[0].available);
    assert!(!st.destinations[0].available);
    assert_eq!(st.flows[0].state, FlowState::Unavailable);
}

#[test]
fn stale_synced_device_references_are_blocked_without_masking_missing_flow_entities() {
    let fx = without_projects();
    let mut source = fx.source.clone();
    source.device_id = "unknown-device-from-peer".into();
    fx.store.put(&source).unwrap();
    let st = status(&fx);
    assert!(!st.sources[0].available);
    assert!(!st.sources[0].wipe_eligible);
    assert_eq!(st.flows[0].state, FlowState::Unavailable);
    assert!(!st.flows[0].runnable);
    assert!(st.flows[0]
        .error
        .as_ref()
        .unwrap()
        .contains("source settings"));
    fx.store
        .delete(EntityKind::Destination, &fx.destination.id)
        .unwrap();
    let err = workspace_status(
        &fx.store,
        &fx.resolver,
        &Catalog::load(&fx.store).unwrap(),
        &context(&fx),
        &FailureMap::new(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("destination"));
}

#[test]
fn project_dependent_destination_is_explicitly_blocked() {
    let fx = without_projects();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut dest = fx.destination.clone();
    dest.path_template = "{project_name}".into();
    fx.store.put(&dest).unwrap();
    let st = status(&fx);
    assert_eq!(st.flows[0].state, FlowState::Error);
    assert!(st.flows[0]
        .error
        .as_deref()
        .unwrap()
        .contains("requires project values"));
}

#[test]
fn invalid_source_template_does_not_scan_device_root() {
    let fx = without_projects();
    fx.write_card_file("private.JPG", b"not in the requested source folder");
    let mut source = fx.source.clone();
    source.path_template = "{project_name}".into();
    fx.store.put(&source).unwrap();
    let st = status(&fx);
    assert_eq!(st.sources[0].file_count, 0);
    assert!(st.sources[0]
        .blocking_reason
        .as_deref()
        .unwrap()
        .contains("Source path"));
    let ctx = resolve_workspace_flow(&fx.store, &fx.resolver, &context(&fx), "flow").unwrap();
    assert!(classify_flow(&ctx, &Catalog::load(&fx.store).unwrap(), None).is_empty());
    assert_eq!(st.flows[0].state, FlowState::Error);
}

#[test]
fn existing_marker_resolves_project_free_backup_folder() {
    let fx = without_projects();
    fx.write_card_file("DCIM/A.JPG", b"photo");
    let mut dest = fx.destination.clone();
    dest.path_template = "{backup_folder}".into();
    dest.use_backup_marker = true;
    dest.subfolder_per_source = false;
    fx.store.put(&dest).unwrap();
    let st = status(&fx);
    assert_eq!(st.flows[0].state, FlowState::Error);
    assert!(st.flows[0]
        .error
        .as_deref()
        .unwrap()
        .contains("backup folder"));
    crate::paths::ensure_backup_folder(fx.card_dir.path(), "Existing").unwrap();
    assert_eq!(status(&fx).flows[0].state, FlowState::Pending);
    let ctx = resolve_workspace_flow(&fx.store, &fx.resolver, &context(&fx), "flow").unwrap();
    assert_eq!(
        ctx.target_for_project("A.JPG", None).unwrap(),
        Some("Existing/A.JPG".into())
    );
}

#[test]
fn invalid_context_ids_never_fall_back_to_another_space_or_project() {
    let fx = Fixture::new();
    let mut invalid = context(&fx);
    invalid.project_id = Some("missing".into());
    assert!(invalid
        .load(&fx.store)
        .unwrap_err()
        .to_string()
        .contains("not found"));
    let mut other = fx.project.clone();
    other.space_id = "other".into();
    fx.store.put(&other).unwrap();
    invalid.project_id = Some(other.id);
    assert!(invalid
        .load(&fx.store)
        .unwrap_err()
        .to_string()
        .contains("must belong"));
    invalid.project_id = None;
    invalid.space_id = "missing".into();
    assert!(invalid.load(&fx.store).is_err());
    let mut flow = fx.flow.clone();
    flow.space_id = "other".into();
    fx.store.put(&flow).unwrap();
    assert!(
        resolve_workspace_flow(&fx.store, &fx.resolver, &context(&fx), "flow")
            .err()
            .unwrap()
            .to_string()
            .contains("must belong")
    );
}
