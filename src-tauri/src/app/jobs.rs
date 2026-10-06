use super::AppCore;
use crate::domain::{Destination, DestinationKind, Device, Flow, Source};
use crate::plan::{resolve_workspace_flow, workspace_status, Catalog, FlowState, WorkspaceContext};
use crate::transfer::{
    run_workspace_check, run_workspace_transfer, AnalysisContext, ConflictQueue, JobKind, JobSpec,
    ResourceClaim,
};
use crate::wipe::{wipe, WipeMethod};
use std::sync::Arc;

impl AppCore {
    pub fn run_flow(&self, project_id: &str, flow_id: &str) -> Result<String, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        self.run_workspace_flow(&context, flow_id)
    }

    pub fn run_workspace_flow(
        &self,
        context: &WorkspaceContext,
        flow_id: &str,
    ) -> Result<String, String> {
        self.enqueue_transfer(context, flow_id, ConflictQueue::new())
    }

    fn enqueue_transfer(
        &self,
        context: &WorkspaceContext,
        flow_id: &str,
        queue: Arc<ConflictQueue>,
    ) -> Result<String, String> {
        let ctx = resolve_workspace_flow(&self.store, self.resolver.as_ref(), context, flow_id)
            .map_err(|e| e.to_string())?;
        if ctx.destination.kind == DestinationKind::App {
            return Err("App destinations can only be triggered manually".into());
        }
        if let Some(error) = &ctx.config_error {
            return Err(error.clone());
        }
        if ctx.source_root.is_none() || ctx.dest_root.is_none() {
            return Err("Connect the source and destination devices to run".into());
        }
        let (store, resolver, failures) = (
            self.store.clone(),
            self.resolver.clone(),
            self.failures.clone(),
        );
        let (context, flow) = (context.clone(), flow_id.to_string());
        let destination_path = format!(
            "destination-path:{}:{}",
            ctx.dest_device.id, ctx.dest_folder_rel
        );
        let analysis = AnalysisContext::from_flow(&ctx);
        Ok(self.transfers.enqueue_with_analysis(
            JobSpec {
                key: flow_id.to_string(),
                label: ctx.label(),
                resources: vec![
                    ResourceClaim::shared(format!("device:{}", ctx.source_device.id)),
                    ResourceClaim::shared(format!("device:{}", ctx.dest_device.id)),
                    ResourceClaim::exclusive(destination_path),
                ],
                kind: JobKind::Transfer,
                queue: Some(queue),
                work: Box::new(move |handle| {
                    run_workspace_transfer(
                        &store,
                        resolver.as_ref(),
                        &context,
                        &flow,
                        handle,
                        &failures,
                    )
                }),
            },
            Some(analysis),
        ))
    }

    /// Queues every flow of the project that has something to copy (or to retry).
    pub fn run_all(&self, project_id: &str) -> Result<Vec<String>, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        self.run_workspace_all(&context)
    }

    pub fn run_workspace_all(&self, context: &WorkspaceContext) -> Result<Vec<String>, String> {
        let runnable = self.runnable_flows(context, None)?;
        let queue = ConflictQueue::new();
        runnable
            .iter()
            .map(|id| self.enqueue_transfer(context, id, queue.clone()))
            .collect()
    }

    /// Queues the runnable incoming flows of one destination as a single conflict queue.
    pub fn run_workspace_destination(
        &self,
        context: &WorkspaceContext,
        destination_id: &str,
    ) -> Result<Vec<String>, String> {
        self.folder_destination(context, destination_id)?;
        let runnable = self.runnable_flows(context, Some(destination_id))?;
        let queue = ConflictQueue::new();
        runnable
            .iter()
            .map(|id| self.enqueue_transfer(context, id, queue.clone()))
            .collect()
    }

    fn runnable_flows(
        &self,
        context: &WorkspaceContext,
        destination_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let failures = self.failures.lock().clone();
        let status = workspace_status(
            &self.store,
            self.resolver.as_ref(),
            &catalog,
            context,
            &failures,
        )
        .map_err(|e| e.to_string())?;
        let mut runnable = Vec::new();
        for f in status.flows.iter().filter(|f| {
            matches!(f.state, FlowState::Pending | FlowState::Error) && f.to_transfer + f.failed > 0
        }) {
            let ctx =
                resolve_workspace_flow(&self.store, self.resolver.as_ref(), context, &f.flow_id)
                    .map_err(|e| e.to_string())?;
            if destination_id.is_some_and(|id| ctx.flow.destination_id != id) {
                continue;
            }
            if ctx.destination.kind == DestinationKind::Folder
                && ctx.source_root.is_some()
                && ctx.dest_root.is_some()
                && ctx.config_error.is_none()
            {
                runnable.push(f.flow_id.clone());
            }
        }
        Ok(runnable)
    }

    fn folder_destination(
        &self,
        context: &WorkspaceContext,
        destination_id: &str,
    ) -> Result<Destination, String> {
        let destination: Destination = self
            .store
            .get(destination_id)
            .map_err(|e| e.to_string())?
            .ok_or("destination not found")?;
        if destination.space_id != context.space_id {
            return Err("Destination must belong to the workspace space".into());
        }
        if destination.kind != DestinationKind::Folder {
            return Err("Only folder destinations can be run or checked".into());
        }
        Ok(destination)
    }

    /// Queues a read-only hash check for every connected, valid incoming flow of a
    /// folder destination.
    pub fn check_workspace_destination(
        &self,
        context: &WorkspaceContext,
        destination_id: &str,
    ) -> Result<Vec<String>, String> {
        self.folder_destination(context, destination_id)?;
        context.load(&self.store).map_err(|e| e.to_string())?;
        let flows: Vec<Flow> = self
            .store
            .list_by("space_id", &context.space_id)
            .map_err(|e| e.to_string())?;
        let mut ids = Vec::new();
        for flow in flows.iter().filter(|f| f.destination_id == destination_id) {
            let ctx =
                resolve_workspace_flow(&self.store, self.resolver.as_ref(), context, &flow.id)
                    .map_err(|e| e.to_string())?;
            if ctx.config_error.is_some()
                || !ctx.source_path_valid
                || ctx.source_root.is_none()
                || ctx.dest_root.is_none()
            {
                continue;
            }
            let (store, resolver) = (self.store.clone(), self.resolver.clone());
            let (context, flow_id) = (context.clone(), flow.id.clone());
            let destination_path = format!(
                "destination-path:{}:{}",
                ctx.dest_device.id, ctx.dest_folder_rel
            );
            let analysis = AnalysisContext::from_flow(&ctx);
            ids.push(self.transfers.enqueue_with_analysis(
                JobSpec {
                    key: format!("check:{}", flow.id),
                    label: ctx.label(),
                    resources: vec![
                        ResourceClaim::shared(format!("device:{}", ctx.source_device.id)),
                        ResourceClaim::shared(format!("device:{}", ctx.dest_device.id)),
                        ResourceClaim::shared(destination_path),
                    ],
                    kind: JobKind::Check,
                    queue: None,
                    work: Box::new(move |handle| {
                        run_workspace_check(&store, resolver.as_ref(), &context, &flow_id, handle)
                    }),
                },
                Some(analysis),
            ));
        }
        if ids.is_empty() {
            return Err("Connect the source and destination devices to check".into());
        }
        Ok(ids)
    }

    pub fn start_wipe(
        &self,
        project_id: &str,
        source_id: &str,
        method: WipeMethod,
    ) -> Result<String, String> {
        let source: Source = self
            .store
            .get(source_id)
            .map_err(|e| e.to_string())?
            .ok_or("source not found")?;
        let device: Device = self
            .store
            .get(&source.device_id)
            .map_err(|e| e.to_string())?
            .ok_or("Select a device in source settings before wiping")?;
        let plan =
            crate::wipe::plan_wipe(&self.store, self.resolver.as_ref(), project_id, source_id)?;
        if let Some(reason) = plan.reason {
            return Err(reason);
        }
        let (store, resolver) = (self.store.clone(), self.resolver.clone());
        let (project, source) = (project_id.to_string(), source_id.to_string());
        Ok(self.transfers.enqueue(JobSpec {
            key: format!("wipe:{source_id}"),
            label: format!("Wipe {}", device.name),
            resources: vec![ResourceClaim::exclusive(format!("device:{}", device.id))],
            kind: JobKind::Wipe,
            queue: None,
            work: Box::new(move |handle| {
                wipe(&store, resolver.as_ref(), &project, &source, method, handle)
            }),
        }))
    }

    pub fn start_workspace_wipe(
        &self,
        source_id: &str,
        method: WipeMethod,
    ) -> Result<String, String> {
        let source: Source = self
            .store
            .get(source_id)
            .map_err(|e| e.to_string())?
            .ok_or("source not found")?;
        let device: Device = self
            .store
            .get(&source.device_id)
            .map_err(|e| e.to_string())?
            .ok_or("Select a device in source settings before wiping")?;
        let plan =
            crate::wipe::plan_workspace_wipe(&self.store, self.resolver.as_ref(), source_id)?;
        if let Some(reason) = plan.reason {
            return Err(reason);
        }
        let (store, resolver, source_id) = (
            self.store.clone(),
            self.resolver.clone(),
            source_id.to_string(),
        );
        Ok(self.transfers.enqueue(JobSpec {
            key: format!("wipe:{source_id}"),
            label: format!("Wipe {}", device.name),
            resources: vec![ResourceClaim::exclusive(format!("device:{}", device.id))],
            kind: JobKind::Wipe,
            queue: None,
            work: Box::new(move |handle| {
                crate::wipe::wipe_workspace(&store, resolver.as_ref(), &source_id, method, handle)
            }),
        }))
    }
}
