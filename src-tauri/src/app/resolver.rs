use crate::plan::RootResolver;
use crate::store::Store;
use std::path::PathBuf;
use std::sync::Arc;

/// Resolves devices through this computer's device mappings.
pub struct DeviceResolver(pub Arc<Store>);

impl RootResolver for DeviceResolver {
    fn device_root(&self, device_id: &str) -> Option<PathBuf> {
        crate::devices::resolve_root(&self.0, device_id)
    }
}
