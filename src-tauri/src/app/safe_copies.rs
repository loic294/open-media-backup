use super::AppCore;
use crate::domain::{Destination, DestinationKind, Device, DeviceMapping, FileRule, Source};
use crate::plan::{assess_workspace_device, Catalog, FinalSet, SafeCopyFile, WorkspaceContext};
use crate::rules::RuleSet;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SourceSafeCopyDetails {
    pub source_id: String,
    pub device_id: String,
    pub device_name: String,
    pub required_copies: Option<u32>,
    pub safe_copies: usize,
    pub wipe_eligible: bool,
    pub blocking_reason: Option<String>,
    pub editable: bool,
    pub files: Vec<SafeCopyFile>,
}

impl AppCore {
    fn safe_copy_source(
        &self,
        context: &WorkspaceContext,
        source_id: &str,
    ) -> Result<(Source, Device), String> {
        context
            .load(&self.store)
            .map_err(|error| error.to_string())?;
        let source: Source = self
            .store
            .get(source_id)
            .map_err(|error| error.to_string())?
            .ok_or("source not found")?;
        if source.space_id != context.space_id {
            return Err("Source must belong to the workspace space".into());
        }
        let device = self
            .store
            .get(&source.device_id)
            .map_err(|error| error.to_string())?
            .ok_or("Select a device for this source in its settings")?;
        Ok((source, device))
    }

    fn safe_copy_editable(&self, device_id: &str) -> Result<bool, String> {
        Ok(self
            .store
            .get::<DeviceMapping>(&format!("{device_id}@{}", self.store.computer_id()))
            .map_err(|error| error.to_string())?
            .is_some_and(|mapping| {
                mapping.device_id == device_id
                    && mapping.computer_id == self.store.computer_id()
                    && !mapping.root_path.trim().is_empty()
            }))
    }

    pub fn save_source_safe_copy_rules(
        &self,
        context: &WorkspaceContext,
        source_id: &str,
        rules: Vec<FileRule>,
    ) -> Result<(), String> {
        let (mut source, device) = self.safe_copy_source(context, source_id)?;
        if !self.safe_copy_editable(&device.id)? {
            return Err(
                "Safe-copy rules can only be edited for a device mapped on this computer".into(),
            );
        }
        RuleSet::compile(&rules).map_err(|error| error.to_string())?;
        self.transfers
            .with_idle_resource(&format!("device:{}", device.id), || {
                source.safe_copy_rules = rules;
                self.store.put(&source).map_err(|error| error.to_string())
            })
    }

    pub fn source_safe_copy_details(
        &self,
        context: &WorkspaceContext,
        source_id: &str,
    ) -> Result<SourceSafeCopyDetails, String> {
        let (source, device) = self.safe_copy_source(context, source_id)?;
        let (space, project) = context
            .load(&self.store)
            .map_err(|error| error.to_string())?;
        let catalog = Catalog::load(&self.store).map_err(|error| error.to_string())?;
        let finals =
            FinalSet::load_for_space(&self.store, &space.id).map_err(|error| error.to_string())?;
        let sources = self
            .store
            .list_by::<Source>("space_id", &space.id)
            .map_err(|error| error.to_string())?;
        let mut assessment = assess_workspace_device(
            self.resolver.as_ref(),
            &catalog,
            &space,
            project.as_ref(),
            &device,
            &sources,
            &finals.targets(),
        )
        .into_iter()
        .find(|(candidate, _)| candidate.id == source.id)
        .map(|(_, assessment)| assessment)
        .ok_or("source not found in device assessment")?;
        let app_targets: std::collections::HashSet<_> = self
            .store
            .list_by::<Destination>("space_id", &space.id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|destination| destination.kind == DestinationKind::App)
            .map(|destination| destination.id)
            .collect();
        for file in &mut assessment.device_files {
            for target in finals.targets() {
                let missing = file.reasons.iter().any(|reason| {
                    reason
                        == &format!(
                            "{}: missing a verified copy; transfer or check this destination",
                            target.device.name
                        )
                });
                if missing
                    && !app_targets.contains(&target.device.id)
                    && self
                        .resolver
                        .device_root(&target.device.id)
                        .is_none_or(|root| !root.exists())
                {
                    file.reasons.push(format!(
                        "{} is offline; reconnect it to transfer or check the missing copy",
                        target.device.name
                    ));
                }
            }
            file.reasons.sort();
            file.reasons.dedup();
        }
        let wipe =
            crate::wipe::plan_workspace_wipe(&self.store, self.resolver.as_ref(), &source.id)?;
        Ok(SourceSafeCopyDetails {
            source_id: source.id,
            device_id: device.id.clone(),
            device_name: device.name,
            required_copies: assessment.status.required_copies,
            safe_copies: assessment.status.safe_copies,
            wipe_eligible: wipe.eligible,
            blocking_reason: wipe.reason,
            editable: self.safe_copy_editable(&device.id)?,
            files: assessment.device_files,
        })
    }
}
