use super::AppCore;
use crate::domain::{DestinationKind, Device, Source};
use crate::plan::{resolve_workspace_flow, workspace_status, Catalog, FlowState, WorkspaceContext};
use crate::transfer::{run_workspace_transfer, JobSpec};
use crate::wipe::{wipe, WipeMethod};

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
        Ok(self.transfers.enqueue(JobSpec {
            key: flow_id.to_string(),
            label: ctx.label(),
            devices: vec![ctx.source_device.id.clone(), ctx.dest_device.id.clone()],
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
        }))
    }

    /// Queues every flow of the project that has something to copy (or to retry).
    pub fn run_all(&self, project_id: &str) -> Result<Vec<String>, String> {
        let context =
            WorkspaceContext::for_project(&self.store, project_id).map_err(|e| e.to_string())?;
        self.run_workspace_all(&context)
    }

    pub fn run_workspace_all(&self, context: &WorkspaceContext) -> Result<Vec<String>, String> {
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
            if ctx.destination.kind == DestinationKind::Folder
                && ctx.source_root.is_some()
                && ctx.dest_root.is_some()
                && ctx.config_error.is_none()
            {
                runnable.push(f.flow_id.clone());
            }
        }
        runnable
            .iter()
            .map(|id| self.run_workspace_flow(context, id))
            .collect()
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
            devices: vec![device.id],
            work: Box::new(move |handle| {
                wipe(&store, resolver.as_ref(), &project, &source, method, handle)
            }),
        }))
    }
}
