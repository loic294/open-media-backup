//! Durable telemetry is deliberately outside entities, field clocks, notifications and oplog.
use super::{Store, StoreError, StoreResult};
use crate::transfer::metrics::{AnalysisKind, AnalysisState};
use crate::transfer::{AnalysisContext, AnalysisJob, AnalysisMetrics, AnalysisPhase};
use rusqlite::{params, params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AnalysisFilter {
    pub space_id: Option<String>,
    pub since: Option<i64>,
    pub pair_id: Option<String>,
}

impl AnalysisFilter {
    fn validate(&self) -> StoreResult<()> {
        if self.since.is_some_and(|since| since < 0)
            || self
                .space_id
                .as_ref()
                .is_some_and(|id| id.trim().is_empty())
        {
            return Err(StoreError::Invalid("Invalid speed analysis filter".into()));
        }
        if let Some(pair) = &self.pair_id {
            let ids: [String; 5] = serde_json::from_str(pair).map_err(|_| {
                StoreError::Invalid("pair_id must be a JSON tuple of five IDs".into())
            })?;
            if ids.iter().any(|id| id.trim().is_empty()) || serde_json::to_string(&ids)? != *pair {
                return Err(StoreError::Invalid("Invalid canonical pair_id".into()));
            }
        }
        Ok(())
    }

    fn sql(&self) -> (String, Vec<Value>) {
        let mut clauses = vec!["1=1".to_string()];
        let mut values = vec![];
        for (column, value) in [
            ("space_id", self.space_id.as_ref()),
            ("pair_id", self.pair_id.as_ref()),
        ] {
            if let Some(value) = value {
                clauses.push(format!("{column} = ?"));
                values.push(Value::Text(value.clone()));
            }
        }
        if let Some(since) = self.since {
            clauses.push("created_at >= ?".into());
            values.push(Value::Integer(since));
        }
        (clauses.join(" AND "), values)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalysisJobsRequest {
    #[serde(flatten)]
    pub filter: AnalysisFilter,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisJobPage {
    pub jobs: Vec<AnalysisJob>,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AnalysisTotals {
    pub metrics: AnalysisMetrics,
    pub completed_transfer_jobs: u64,
    pub completed_check_jobs: u64,
    pub failed_jobs: u64,
    pub cancelled_jobs: u64,
    pub interrupted_jobs: u64,
    pub avg_copy_bps: Option<f64>,
    pub effective_bps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisPair {
    pub context: AnalysisContext,
    pub totals: AnalysisTotals,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AnalysisSummary {
    pub pairs: Vec<AnalysisPair>,
    pub totals: AnalysisTotals,
}

#[derive(Default)]
struct Aggregate {
    totals: AnalysisTotals,
    transfer_active_secs: f64,
}

impl Aggregate {
    fn add(&mut self, job: &AnalysisJob) {
        match job.state {
            AnalysisState::Done if job.kind == AnalysisKind::Transfer => {
                self.totals.completed_transfer_jobs += 1;
                self.totals.metrics.add(&job.metrics);
                self.transfer_active_secs += job.metrics.active_secs();
            }
            AnalysisState::Done => {
                self.totals.completed_check_jobs += 1;
                // Standalone checks contribute only separate read I/O and read time.
                self.totals.metrics.add(&AnalysisMetrics {
                    source_check_secs: job.metrics.source_check_secs,
                    destination_check_secs: job.metrics.destination_check_secs,
                    source_check_bytes: job.metrics.source_check_bytes,
                    destination_check_bytes: job.metrics.destination_check_bytes,
                    ..Default::default()
                });
            }
            AnalysisState::Failed => self.totals.failed_jobs += 1,
            AnalysisState::Cancelled => self.totals.cancelled_jobs += 1,
            AnalysisState::Interrupted => self.totals.interrupted_jobs += 1,
            _ => {}
        }
    }

    fn finish(mut self) -> AnalysisTotals {
        fn rate(bytes: u64, secs: f64) -> Option<f64> {
            (bytes > 0 && secs > 0.0).then(|| bytes as f64 / secs)
        }
        self.totals.avg_copy_bps = rate(
            self.totals.metrics.copy_bytes,
            self.totals.metrics.copy_secs,
        );
        self.totals.effective_bps = rate(
            self.totals.metrics.committed_bytes,
            self.transfer_active_secs,
        );
        self.totals
    }
}

impl Store {
    fn check_analysis_available(&self) -> StoreResult<()> {
        if let Some(error) = &self.analysis_recovery_error {
            return Err(StoreError::Invalid(error.clone()));
        }
        Ok(())
    }

    pub fn save_analysis_job(&self, job: &AnalysisJob) -> StoreResult<()> {
        self.check_analysis_available()?;
        save(&self.conn.lock(), job)
    }

    pub fn list_speed_analysis_jobs(
        &self,
        req: &AnalysisJobsRequest,
    ) -> StoreResult<AnalysisJobPage> {
        self.check_analysis_available()?;
        req.filter.validate()?;
        if req.offset < 0 || !(0..=100).contains(&req.limit) {
            return Err(StoreError::Invalid(
                "offset must be nonnegative and limit must be 0..=100".into(),
            ));
        }
        let (condition, mut values) = req.filter.sql();
        let conn = self.conn.lock();
        let total: u64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM speed_analysis WHERE {condition}"),
            params_from_iter(&values),
            |r| r.get(0),
        )?;
        values.extend([Value::Integer(req.limit), Value::Integer(req.offset)]);
        let mut stmt = conn.prepare(&format!(
            "SELECT data FROM speed_analysis WHERE {condition} ORDER BY created_at DESC, id ASC LIMIT ? OFFSET ?"
        ))?;
        let rows = stmt.query_map(params_from_iter(values), |r| r.get::<_, String>(0))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(serde_json::from_str(&row?)?);
        }
        Ok(AnalysisJobPage { jobs, total })
    }

    pub fn get_speed_analysis(&self, filter: &AnalysisFilter) -> StoreResult<AnalysisSummary> {
        self.check_analysis_available()?;
        filter.validate()?;
        let (condition, values) = filter.sql();
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT data FROM speed_analysis WHERE {condition} AND state IN ('done','failed','cancelled','interrupted') ORDER BY created_at ASC, id ASC"
        ))?;
        let rows = stmt.query_map(params_from_iter(values), |r| r.get::<_, String>(0))?;
        let mut total = Aggregate::default();
        let mut pairs: BTreeMap<String, (AnalysisContext, Aggregate)> = BTreeMap::new();
        for row in rows {
            let job: AnalysisJob = serde_json::from_str(&row?)?;
            total.add(&job);
            let entry = pairs
                .entry(job.context.pair_id.clone())
                .or_insert_with(|| (job.context.clone(), Aggregate::default()));
            entry.0 = job.context.clone();
            entry.1.add(&job);
        }
        Ok(AnalysisSummary {
            pairs: pairs
                .into_values()
                .map(|(context, aggregate)| AnalysisPair {
                    context,
                    totals: aggregate.finish(),
                })
                .collect(),
            totals: total.finish(),
        })
    }
}

fn save(conn: &Connection, job: &AnalysisJob) -> StoreResult<()> {
    let state = serde_json::to_value(job.state)?;
    conn.execute(
        "INSERT INTO speed_analysis (id,pair_id,space_id,created_at,state,data) VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(id) DO UPDATE SET state=excluded.state,data=excluded.data
         WHERE speed_analysis.state NOT IN ('done','failed','cancelled','interrupted')",
        params![job.id, job.context.pair_id, job.context.space_id, job.created_at, state.as_str(), serde_json::to_string(job)?],
    )?;
    Ok(())
}

pub(super) fn recover_interrupted(conn: &Connection) -> StoreResult<()> {
    let transaction = conn.unchecked_transaction()?;
    let snapshots = {
        let mut stmt = transaction.prepare(
            "SELECT data FROM speed_analysis WHERE state NOT IN ('done','failed','cancelled','interrupted')",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for snapshot in snapshots {
        let mut job: AnalysisJob = serde_json::from_str(&snapshot)?;
        job.state = AnalysisState::Interrupted;
        job.phase = AnalysisPhase::Finished;
        job.finished_at = Some(job.updated_at);
        save(&transaction, &job)?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::metrics::tests::context;
    use crate::transfer::metrics::PhaseTracker;
    use crate::transfer::{JobKind, JobState};

    fn job(id: &str, at: i64, state: AnalysisState, kind: AnalysisKind) -> AnalysisJob {
        let mut t = PhaseTracker::new(id.into(), context(), JobKind::Transfer);
        t.refresh(JobState::Done, 0);
        let mut j = t.job;
        j.created_at = at;
        j.updated_at = at + 100;
        j.finished_at = state.is_terminal().then_some(j.updated_at);
        j.state = state;
        j.kind = kind;
        j
    }

    #[test]
    fn weighted_success_only_rates_and_check_only_pairs() {
        let store = Store::open_in_memory().unwrap();
        for (id, bytes, secs) in [("a", 100, 1.0), ("b", 900, 9.0)] {
            let mut j = job(id, 1, AnalysisState::Done, AnalysisKind::Transfer);
            j.metrics = AnalysisMetrics {
                copy_bytes: bytes,
                committed_bytes: bytes / 2,
                copy_secs: secs,
                other_secs: secs,
                ..Default::default()
            };
            store.save_analysis_job(&j).unwrap();
            store.save_analysis_job(&j).unwrap();
        }
        for state in [
            AnalysisState::Failed,
            AnalysisState::Cancelled,
            AnalysisState::Interrupted,
            AnalysisState::Running,
        ] {
            let mut j = job(&format!("{state:?}"), 2, state, AnalysisKind::Transfer);
            j.metrics.copy_bytes = 100_000;
            store.save_analysis_job(&j).unwrap();
        }
        let mut check = job("check", 3, AnalysisState::Done, AnalysisKind::Check);
        check.context.pair_id = r#"["s","x","b","c","d"]"#.into();
        check.metrics.source_check_secs = 30.0;
        check.metrics.destination_check_bytes = 200;
        check.metrics.other_secs = 100.0;
        check.metrics.copy_bytes = 9999;
        store.save_analysis_job(&check).unwrap();
        let s = store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap();
        assert_eq!(s.pairs.len(), 2);
        assert_eq!(s.totals.completed_transfer_jobs, 2);
        assert_eq!(s.totals.completed_check_jobs, 1);
        assert_eq!(s.totals.failed_jobs, 1);
        assert_eq!(s.totals.cancelled_jobs, 1);
        assert_eq!(s.totals.interrupted_jobs, 1);
        assert_eq!(s.totals.avg_copy_bps, Some(100.0));
        assert_eq!(s.totals.effective_bps, Some(25.0));
        assert_eq!(s.totals.metrics.other_secs, 10.0);
        assert_eq!(s.totals.metrics.source_check_secs, 30.0);
        assert_eq!(s.pairs[1].totals.avg_copy_bps, None);
        assert_eq!(s.pairs[1].totals.effective_bps, None);
        let failed = job("only-failed", 4, AnalysisState::Failed, AnalysisKind::Check);
        let mut failed = failed;
        failed.context.pair_id = r#"["s","z","b","c","d"]"#.into();
        store.save_analysis_job(&failed).unwrap();
        assert_eq!(
            store
                .get_speed_analysis(&AnalysisFilter::default())
                .unwrap()
                .pairs
                .len(),
            3
        );
    }

    #[test]
    fn filters_pagination_validation_and_terminal_immutability() {
        let store = Store::open_in_memory().unwrap();
        for (id, at) in [("a", 10), ("b", 20), ("c", 20)] {
            store
                .save_analysis_job(&job(id, at, AnalysisState::Done, AnalysisKind::Transfer))
                .unwrap();
        }
        let req = AnalysisJobsRequest {
            filter: AnalysisFilter {
                since: Some(20),
                space_id: Some("s".into()),
                pair_id: Some(context().pair_id),
            },
            offset: 1,
            limit: 1,
        };
        let page = store.list_speed_analysis_jobs(&req).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.jobs[0].id, "c");
        assert_eq!(
            store
                .get_speed_analysis(&req.filter)
                .unwrap()
                .totals
                .completed_transfer_jobs,
            2
        );
        let mut stale = job("a", 10, AnalysisState::Running, AnalysisKind::Transfer);
        stale.metrics.copy_bytes = 1000;
        store.save_analysis_job(&stale).unwrap();
        assert_eq!(
            store
                .get_speed_analysis(&AnalysisFilter::default())
                .unwrap()
                .totals
                .metrics
                .copy_bytes,
            0
        );
        for limit in [-1, 101] {
            assert!(store
                .list_speed_analysis_jobs(&AnalysisJobsRequest {
                    limit,
                    ..req.clone()
                })
                .is_err());
        }
        assert!(store
            .list_speed_analysis_jobs(&AnalysisJobsRequest { offset: -1, ..req })
            .is_err());
        for filter in [
            AnalysisFilter {
                since: Some(-1),
                ..Default::default()
            },
            AnalysisFilter {
                pair_id: Some("not-json".into()),
                ..Default::default()
            },
        ] {
            assert!(store.get_speed_analysis(&filter).is_err());
        }
        assert!(store
            .ops_since(&Default::default(), 100)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn restart_recovers_last_checkpoint_without_downtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let mut running = job("running", 20, AnalysisState::Paused, AnalysisKind::Transfer);
        running.metrics.copy_secs = 2.0;
        running.metrics.copy_bytes = 100;
        {
            let store = Store::open(&path).unwrap();
            store.save_analysis_job(&running).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let page = store
            .list_speed_analysis_jobs(&AnalysisJobsRequest {
                filter: Default::default(),
                offset: 0,
                limit: 100,
            })
            .unwrap();
        let j = &page.jobs[0];
        assert_eq!(j.state, AnalysisState::Interrupted);
        assert_eq!(j.finished_at, Some(running.updated_at));
        assert_eq!(j.updated_at, running.updated_at);
        assert_eq!(j.metrics, running.metrics);
        let summary = store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap();
        assert_eq!(summary.totals.interrupted_jobs, 1);
        assert_eq!(summary.totals.metrics, AnalysisMetrics::default());
    }

    #[test]
    fn zero_adopted_skipped_only_jobs_have_no_speed_and_keep_name_snapshots() {
        let store = Store::open_in_memory().unwrap();
        let mut adopted = job("first", 1, AnalysisState::Done, AnalysisKind::Transfer);
        adopted.metrics.adopted_files = 2;
        adopted.metrics.skipped_files = 1;
        adopted.metrics.other_secs = 2.0;
        store.save_analysis_job(&adopted).unwrap();
        let mut renamed = job("second", 2, AnalysisState::Done, AnalysisKind::Transfer);
        renamed.context.source_name = "Renamed / deleted source".into();
        store.save_analysis_job(&renamed).unwrap();
        let summary = store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap();
        assert_eq!(summary.pairs.len(), 1);
        assert_eq!(
            summary.pairs[0].context.source_name,
            renamed.context.source_name
        );
        assert_eq!(summary.totals.metrics.adopted_files, 2);
        assert_eq!(summary.totals.metrics.skipped_files, 1);
        assert_eq!(summary.totals.avg_copy_bps, None);
        assert_eq!(summary.totals.effective_bps, None);
        let page = store
            .list_speed_analysis_jobs(&AnalysisJobsRequest {
                filter: Default::default(),
                offset: 0,
                limit: 100,
            })
            .unwrap();
        assert_eq!(page.jobs[1].context, adopted.context);
        let none = AnalysisFilter {
            space_id: Some("other".into()),
            ..Default::default()
        };
        assert!(store.get_speed_analysis(&none).unwrap().pairs.is_empty());
        assert_eq!(
            store
                .list_speed_analysis_jobs(&AnalysisJobsRequest {
                    filter: none,
                    offset: 0,
                    limit: 100,
                })
                .unwrap()
                .total,
            0
        );
        let no_copy_time = Aggregate {
            totals: AnalysisTotals {
                metrics: AnalysisMetrics {
                    copy_bytes: 1,
                    committed_bytes: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            transfer_active_secs: 0.0,
        }
        .finish();
        assert_eq!(no_copy_time.avg_copy_bps, None);
        assert_eq!(no_copy_time.effective_bps, None);
    }

    #[test]
    fn migration_preserves_existing_catalog_and_indexes_queries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.sqlite");
        let expected_ops;
        {
            let store = Store::open(&path).unwrap();
            store
                .put(&crate::domain::Space {
                    id: "existing".into(),
                    ..Default::default()
                })
                .unwrap();
            expected_ops = store.ops_since(&Default::default(), 100).unwrap().len();
            store
                .conn
                .lock()
                .execute_batch("DROP TABLE speed_analysis; PRAGMA user_version = 1;")
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert!(store
            .get::<crate::domain::Space>("existing")
            .unwrap()
            .is_some());
        assert_eq!(
            store
                .conn
                .lock()
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            2
        );
        let plan: String = store.conn.lock().query_row(
            "EXPLAIN QUERY PLAN SELECT data FROM speed_analysis WHERE space_id=?1 AND pair_id=?2 AND created_at>=?3 ORDER BY created_at DESC,id ASC LIMIT 10",
            params!["s", context().pair_id, 0],
            |r| r.get(3),
        ).unwrap();
        assert!(plan.contains("speed_analysis_space_pair_time"), "{plan}");
        store
            .save_analysis_job(&job(
                "analysis",
                1,
                AnalysisState::Done,
                AnalysisKind::Check,
            ))
            .unwrap();
        assert_eq!(
            store.ops_since(&Default::default(), 100).unwrap().len(),
            expected_ops
        );
    }

    #[test]
    fn storage_failure_is_explicit_without_failing_backup_work() {
        use crate::transfer::{JobKind, JobSpec, JobState, TransferManager};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .conn
            .lock()
            .execute_batch("DROP TABLE speed_analysis")
            .unwrap();
        let manager = TransferManager::with_history(
            |_| {},
            crate::transfer::PowerController::new(false, |_| {}),
            Some(store.clone()),
        );
        manager.enqueue_with_analysis(
            JobSpec {
                key: "backup".into(),
                label: "".into(),
                kind: JobKind::Transfer,
                resources: vec![],
                queue: None,
                work: Box::new(|_| Ok(())),
            },
            Some(context()),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while manager.is_busy() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(manager.jobs()[0].state, JobState::Done);
        assert!(manager
            .analysis_storage_error()
            .unwrap()
            .contains("no such table"));
        assert!(store
            .get_speed_analysis(&AnalysisFilter::default())
            .is_err());
        assert!(store
            .list_speed_analysis_jobs(&AnalysisJobsRequest {
                filter: Default::default(),
                offset: 0,
                limit: 100,
            })
            .is_err());
    }

    #[test]
    fn corrupt_analysis_recovery_does_not_disable_the_backup_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt-analysis.sqlite");
        {
            let store = Store::open(&path).unwrap();
            store
                .put(&crate::domain::Space {
                    id: "catalog".into(),
                    ..Default::default()
                })
                .unwrap();
            store
                .save_analysis_job(&job(
                    "corrupt",
                    1,
                    AnalysisState::Running,
                    AnalysisKind::Transfer,
                ))
                .unwrap();
            store
                .conn
                .lock()
                .execute("UPDATE speed_analysis SET data='not-json'", [])
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert!(store
            .get::<crate::domain::Space>("catalog")
            .unwrap()
            .is_some());
        store
            .put(&crate::domain::Space {
                id: "another".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(store
            .get_speed_analysis(&AnalysisFilter::default())
            .unwrap_err()
            .to_string()
            .contains("recover interrupted"));
        assert!(store
            .save_analysis_job(&job("new", 2, AnalysisState::Done, AnalysisKind::Check))
            .is_err());
    }
}
