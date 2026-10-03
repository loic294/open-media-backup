use super::entity::impl_entity;
use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectGranularity {
    #[default]
    Minute,
    Day,
    Year,
}

impl ProjectGranularity {
    pub fn bucket(self, time: i64) -> Option<i64> {
        let date = Utc.timestamp_millis_opt(time).single()?;
        let start = match self {
            Self::Minute => date.with_second(0)?.with_nanosecond(0)?,
            Self::Day => date.date_naive().and_hms_opt(0, 0, 0)?.and_utc(),
            Self::Year => Utc.with_ymd_and_hms(date.year(), 1, 1, 0, 0, 0).single()?,
        };
        Some(start.timestamp_millis())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub values: BTreeMap<String, String>,
    /// Number of verified copies on "final" devices required before a card can be wiped.
    pub final_copies_required: u32,
    pub archived: bool,
    /// Inclusive UTC bounds in Unix milliseconds. Unknown-zone captures remain
    /// unassigned until a timezone policy is configured. Both absent on legacy projects.
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub granularity: ProjectGranularity,
    pub color: String,
}
impl_entity!(Project, Project);

impl Default for Project {
    fn default() -> Self {
        Self {
            id: String::new(),
            space_id: String::new(),
            name: String::new(),
            values: BTreeMap::new(),
            final_copies_required: 2,
            archived: false,
            start_time: None,
            end_time: None,
            granularity: ProjectGranularity::Minute,
            color: "#3b82f6".into(),
        }
    }
}

impl Project {
    pub fn capture_range(&self) -> Result<Option<(i64, i64)>, String> {
        match (self.start_time, self.end_time) {
            (None, None) => Ok(None),
            (Some(start), Some(end)) => {
                let start = self
                    .granularity
                    .bucket(start)
                    .ok_or("Invalid project start time")?;
                let end = self
                    .granularity
                    .bucket(end)
                    .ok_or("Invalid project end time")?;
                if start > end {
                    return Err("Project start time must not be after end time".into());
                }
                Ok(Some((start, end)))
            }
            _ => Err("Project start and end times must both be set".into()),
        }
    }

    pub fn matches_capture_time(&self, capture_time: Option<i64>) -> bool {
        let Some(time) = capture_time.and_then(|t| self.granularity.bucket(t)) else {
            return false;
        };
        self.capture_range()
            .ok()
            .flatten()
            .is_some_and(|(start, end)| start <= time && time <= end)
    }
}

/// Validates the entire space, including updates arriving through sync.
pub fn validate_project_ranges(projects: &[Project], allow_overlap: bool) -> Result<(), String> {
    for project in projects {
        project.capture_range()?;
    }
    if allow_overlap {
        return Ok(());
    }
    for (index, project) in projects.iter().enumerate() {
        let Some((start, end)) = project.capture_range()? else {
            continue;
        };
        for other in &projects[index + 1..] {
            let Some((other_start, other_end)) = other.capture_range()? else {
                continue;
            };
            // Compare using each project's bucket so inclusive day/year endpoints extend
            // through the entire final bucket, not just its first millisecond.
            if project.matches_capture_time(Some(other_start))
                || project.matches_capture_time(Some(other_end))
                || other.matches_capture_time(Some(start))
                || other.matches_capture_time(Some(end))
            {
                return Err(format!(
                    "Project ranges overlap: {} and {}",
                    project.name, other.name
                ));
            }
        }
    }
    Ok(())
}
