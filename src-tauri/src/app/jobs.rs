use super::AppCore;
use crate::domain::{DestinationKind, Device, Source};
use crate::plan::{project_status, resolve_flow, Catalog, FlowState};
use crate::transfer::{run_transfer, JobSpec};
use crate::wipe::{wipe, WipeMethod};

impl AppCore {
    pub fn run_flow(&self, project_id: &str, flow_id: &str) -> Result<String, String> {
        let ctx = resolve_flow(&self.store, self.resolver.as_ref(), project_id, flow_id)
            .map_err(|e| e.to_string())?;
        if ctx.destination.kind == DestinationKind::App {
            return Err("App destinations can only be triggered manually".into());
        }
        let (store, resolver, failures) = (
            self.store.clone(),
            self.resolver.clone(),
            self.failures.clone(),
        );
        let (project, flow) = (project_id.to_string(), flow_id.to_string());
        Ok(self.transfers.enqueue(JobSpec {
            key: flow_id.to_string(),
            label: ctx.label(),
            devices: vec![ctx.source_device.id.clone(), ctx.dest_device.id.clone()],
            work: Box::new(move |handle| {
                run_transfer(
                    &store,
                    resolver.as_ref(),
                    &project,
                    &flow,
                    handle,
                    &failures,
                )
            }),
        }))
    }

    /// Queues every flow of the project that has something to copy (or to retry).
    pub fn run_all(&self, project_id: &str) -> Result<Vec<String>, String> {
        let catalog = Catalog::load(&self.store).map_err(|e| e.to_string())?;
        let status = project_status(
            &self.store,
            self.resolver.as_ref(),
            &catalog,
            project_id,
            &self.failures.lock(),
        )
        .map_err(|e| e.to_string())?;
        status
            .flows
            .iter()
            .filter(|f| {
                matches!(f.state, FlowState::Pending | FlowState::Error)
                    && f.to_transfer + f.failed > 0
            })
            .filter_map(|f| {
                let ctx = resolve_flow(&self.store, self.resolver.as_ref(), project_id, &f.flow_id)
                    .ok()?;
                (ctx.destination.kind == DestinationKind::Folder)
                    .then(|| self.run_flow(project_id, &f.flow_id))
            })
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
            .ok_or("device not found")?;
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
