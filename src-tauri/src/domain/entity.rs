use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Every synced document kind. Stored as JSON documents with per-field LWW clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Space,
    Project,
    Device,
    DeviceMapping,
    Computer,
    Source,
    Destination,
    Flow,
    FileRecord,
    FileCopy,
}

impl EntityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Space => "space",
            Self::Project => "project",
            Self::Device => "device",
            Self::DeviceMapping => "device_mapping",
            Self::Computer => "computer",
            Self::Source => "source",
            Self::Destination => "destination",
            Self::Flow => "flow",
            Self::FileRecord => "file_record",
            Self::FileCopy => "file_copy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
    }
}

/// A typed document stored in the entity store.
pub trait Entity: Serialize + DeserializeOwned + Clone {
    const KIND: EntityKind;
    fn id(&self) -> &str;
}

macro_rules! impl_entity {
    ($ty:ty, $kind:ident) => {
        impl $crate::domain::Entity for $ty {
            const KIND: $crate::domain::EntityKind = $crate::domain::EntityKind::$kind;
            fn id(&self) -> &str {
                &self.id
            }
        }
    };
}
pub(crate) use impl_entity;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips() {
        assert_eq!(EntityKind::parse("file_copy"), Some(EntityKind::FileCopy));
        assert_eq!(EntityKind::FileCopy.as_str(), "file_copy");
        assert_eq!(EntityKind::parse("nope"), None);
    }
}
