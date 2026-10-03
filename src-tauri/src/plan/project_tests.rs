use super::*;
use crate::domain::{
    validate_project_ranges, HashAlgo, Project, ProjectGranularity, ProjectScope, Source, Space,
};
use crate::testing::Fixture;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

fn time(value: &str) -> i64 {
    value.parse::<DateTime<Utc>>().unwrap().timestamp_millis()
}

fn project(id: &str, start: &str, end: &str, granularity: ProjectGranularity) -> Project {
    Project {
        id: id.into(),
        name: id.into(),
        space_id: "space".into(),
        start_time: Some(time(start)),
        end_time: Some(time(end)),
        granularity,
        ..Default::default()
    }
}

#[test]
fn legacy_serialization_defaults_and_new_fields_round_trip() {
    let legacy: Project = serde_json::from_str(r#"{"id":"old","name":"Old"}"#).unwrap();
    assert_eq!(legacy.capture_range().unwrap(), None);
    assert!(!legacy.matches_capture_time(Some(0)));
    assert_eq!(legacy.final_copies_required, 2);
    assert_eq!(legacy.granularity, ProjectGranularity::Minute);
    let space: Space = serde_json::from_str(r#"{"id":"space"}"#).unwrap();
    assert_eq!(space.hash_algo, HashAlgo::Blake3);
    assert!(space.allow_project_overlap);
    assert_eq!(space.temporary_copies_per_final, 0);
    let explicit_xxh64: Space =
        serde_json::from_str(r#"{"id":"space","hash_algo":"xxh64"}"#).unwrap();
    assert_eq!(explicit_xxh64.hash_algo, HashAlgo::Xxh64);
    let mut source: Source = serde_json::from_str(r#"{"id":"source"}"#).unwrap();
    assert_eq!(source.project_scope, ProjectScope::All);
    source.project_scope = ProjectScope::Selected {
        project_ids: vec!["p".into()],
    };
    assert_eq!(
        serde_json::from_value::<Source>(serde_json::to_value(&source).unwrap()).unwrap(),
        source
    );
    let p = project(
        "p",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        ProjectGranularity::Day,
    );
    assert_eq!(
        serde_json::from_value::<Project>(serde_json::to_value(&p).unwrap()).unwrap(),
        p
    );
}

#[test]
fn capture_buckets_are_inclusive_and_missing_capture_never_matches() {
    let mut p = project(
        "p",
        "2026-01-01T12:34:00Z",
        "2026-01-01T12:34:00Z",
        ProjectGranularity::Minute,
    );
    assert!(p.matches_capture_time(Some(time("2026-01-01T12:34:59.999Z"))));
    assert!(!p.matches_capture_time(Some(time("2026-01-01T12:35:00Z"))));
    assert!(!p.matches_capture_time(None));
    p.granularity = ProjectGranularity::Day;
    assert!(p.matches_capture_time(Some(time("2026-01-01T23:59:59.999Z"))));
    p.granularity = ProjectGranularity::Year;
    assert!(p.matches_capture_time(Some(time("2026-12-31T23:59:59.999Z"))));
    assert!(!p.matches_capture_time(Some(time("2027-01-01T00:00:00Z"))));
    assert_eq!(ProjectGranularity::Minute.bucket(-1), Some(-60_000));
    p.end_time = None;
    assert!(p.capture_range().is_err());
}

#[test]
fn overlap_validation_includes_shared_endpoints_and_mixed_granularity() {
    let mut a = project(
        "a",
        "2026-01-01T10:00:00Z",
        "2026-01-01T12:00:00Z",
        ProjectGranularity::Minute,
    );
    let mut b = project(
        "b",
        "2026-01-01T12:00:00Z",
        "2026-01-01T13:00:00Z",
        ProjectGranularity::Minute,
    );
    assert!(validate_project_ranges(&[a.clone(), b.clone()], true).is_ok());
    assert!(validate_project_ranges(&[a.clone(), b.clone()], false).is_err());
    b.start_time = Some(time("2026-01-01T12:01:00Z"));
    assert!(validate_project_ranges(&[a.clone(), b.clone()], false).is_ok());
    a.granularity = ProjectGranularity::Day;
    assert!(validate_project_ranges(&[a, b], false).is_err());
}

#[test]
fn routes_each_matching_project_using_its_values_not_the_selection() {
    let fx = Fixture::new();
    let mut space = fx.space.clone();
    space.variables.push(crate::domain::VariableDef {
        name: "project_name".into(),
        default_value: String::new(),
        required: true,
    });
    fx.store.put(&space).unwrap();
    fx.write_card_file("DCIM/A.JPG", b"x");
    let mut a = project(
        "a",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        ProjectGranularity::Day,
    );
    let mut b = a.clone();
    a.values.insert("client".into(), "Alice".into());
    b.id = "b".into();
    b.name = "b".into();
    b.values.insert("client".into(), "Bob".into());
    fx.store.put_all(&[a, b]).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{client}/{project_name}".into();
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert!(ctx.config_error.is_none());
    let catalog = Catalog::load(&fx.store).unwrap();
    let files = source_files(ctx.source_root.as_deref(), "DCIM", "card", &catalog);
    let captures = HashMap::from([("A.JPG".into(), time("2026-01-01T23:59:59.999Z"))]);
    let planned = classify_files_with_capture_times(&ctx, &catalog, files.clone(), &captures, None);
    assert_eq!(planned.len(), 2);
    let mut targets: Vec<_> = planned
        .iter()
        .map(|file| file.target_path.as_deref().unwrap())
        .collect();
    targets.sort();
    assert_eq!(targets, ["Alice/a/A.JPG", "Bob/b/A.JPG"]);
    let failed_route = planned
        .iter()
        .find(|file| file.project_id.as_deref() == Some("a"))
        .unwrap();
    let failures = HashMap::from([(failed_route.failure_key(), "copy failed".into())]);
    let retry = classify_files_with_capture_times(
        &ctx,
        &catalog,
        files.clone(),
        &captures,
        Some(&failures),
    );
    assert_eq!(
        retry
            .iter()
            .find(|file| file.project_id.as_deref() == Some("a"))
            .unwrap()
            .category,
        Category::Error
    );
    assert_eq!(
        retry
            .iter()
            .find(|file| file.project_id.as_deref() == Some("b"))
            .unwrap()
            .category,
        Category::ToTransfer
    );
    assert!(planned
        .iter()
        .all(|file| file.category == Category::ToTransfer && file.project_id.is_some()));
    let unassigned =
        classify_files_with_capture_times(&ctx, &catalog, files.clone(), &HashMap::new(), None);
    assert_eq!(unassigned.len(), 1);
    assert_eq!(unassigned[0].project_id, None);
    assert_eq!(unassigned[0].category, Category::Ignored);

    let mut source = fx.source.clone();
    source.project_scope = ProjectScope::Selected {
        project_ids: vec!["b".into()],
    };
    fx.store.put(&source).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let selected = classify_files_with_capture_times(&ctx, &catalog, files, &captures, None);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].target_path.as_deref(), Some("Bob/b/A.JPG"));
}

#[test]
fn none_scope_errors_are_visible_for_synced_configuration_and_saves() {
    let fx = Fixture::new();
    let mut source = fx.source.clone();
    source.project_scope = ProjectScope::None;
    fx.store.put(&source).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{ project_name }".into();
    fx.store.put(&destination).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    assert!(ctx
        .config_error
        .unwrap()
        .contains("cannot use project variables"));
}

#[test]
fn synced_overlap_is_a_config_error() {
    let fx = Fixture::new();
    let a = project(
        "a",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        ProjectGranularity::Day,
    );
    let mut b = a.clone();
    b.id = "b".into();
    fx.store.put_all(&[a, b]).unwrap();
    let mut space = fx.space.clone();
    space.allow_project_overlap = false;
    // Simulate sync bypassing UI save validation; planning must still block it.
    fx.store.put(&space).unwrap();
    assert!(resolve_flow(&fx.store, &fx.resolver, "project", "flow")
        .unwrap()
        .config_error
        .unwrap()
        .contains("overlap"));
}

#[test]
fn embedded_offsets_convert_but_unknown_zone_stays_unassigned() {
    let mut capture = crate::metadata::CaptureTime {
        local_datetime: "2026-01-01T12:00:00".into(),
        utc_offset_seconds: None,
        source: crate::metadata::CaptureTimeSource::ExifOriginal,
    };
    assert_eq!(capture_time_ms(&capture).unwrap(), None);
    capture.utc_offset_seconds = Some(3600);
    assert_eq!(
        capture_time_ms(&capture).unwrap(),
        Some(time("2026-01-01T11:00:00Z"))
    );
}

#[test]
fn mounted_image_without_embedded_capture_is_unassigned_despite_mtime() {
    let fx = Fixture::new();
    let path = fx.write_card_file("DCIM/A.PNG", b"");
    ::image::RgbImage::new(1, 1).save(&path).unwrap();
    let project = project(
        "ranged",
        "2000-01-01T00:00:00Z",
        "2100-01-01T00:00:00Z",
        ProjectGranularity::Year,
    );
    fx.store.put(&project).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{project_name}".into();
    fx.store.put(&destination).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let planned = classify_flow(&ctx, &Catalog::load(&fx.store).unwrap(), None);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].capture_time, None);
    assert_eq!(planned[0].project_id, None);
    assert_eq!(planned[0].category, Category::Ignored);
    assert!(planned[0].modified_ms.is_some());
}

#[test]
fn each_project_requires_its_own_destination_path_but_safe_copies_remain_device_based() {
    use crate::domain::{FileCopy, FileRecord, HashAlgo};
    let fx = Fixture::new();
    fx.write_card_file("DCIM/A.JPG", b"x");
    let a = project(
        "a",
        "2026-01-01T00:00:00Z",
        "2026-01-01T00:00:00Z",
        ProjectGranularity::Day,
    );
    let mut b = a.clone();
    b.id = "b".into();
    b.name = "b".into();
    fx.store.put_all(&[a, b]).unwrap();
    let mut destination = fx.destination.clone();
    destination.path_template = "{project_name}".into();
    destination.subfolder_per_source = false;
    fx.store.put(&destination).unwrap();
    let file_id = FileRecord::id_for(HashAlgo::Xxh64, "hash");
    let record = FileRecord {
        id: file_id.clone(),
        size: 1,
        hash: "hash".into(),
        ..Default::default()
    };
    let copy = |device: &str, path: &str| FileCopy {
        id: FileCopy::id_for(&file_id, device, path),
        file_id: file_id.clone(),
        device_id: device.into(),
        path: path.into(),
        ..Default::default()
    };
    fx.store.put(&record).unwrap();
    fx.store
        .put_all(&[copy("card", "DCIM/A.JPG"), copy("nas", "a/A.JPG")])
        .unwrap();
    let catalog = Catalog::load(&fx.store).unwrap();
    let ctx = resolve_flow(&fx.store, &fx.resolver, "project", "flow").unwrap();
    let files = source_files(ctx.source_root.as_deref(), "DCIM", "card", &catalog);
    let capture = HashMap::from([("A.JPG".into(), time("2026-01-01T10:00:00Z"))]);
    let planned = classify_files_with_capture_times(&ctx, &catalog, files, &capture, None);
    assert_eq!(
        planned
            .iter()
            .find(|f| f.project_id.as_deref() == Some("a"))
            .unwrap()
            .category,
        Category::Transferred
    );
    assert_eq!(
        planned
            .iter()
            .find(|f| f.project_id.as_deref() == Some("b"))
            .unwrap()
            .category,
        Category::ToTransfer
    );
    let finals = FinalSet::load(&fx.store).unwrap();
    let assessment = assess_source(
        &fx.resolver,
        &catalog,
        &fx.space,
        &fx.project,
        &fx.card,
        &fx.source,
        &finals.targets(),
    );
    assert!(assessment.status.wipe_eligible);
    assert_eq!(assessment.status.safe_copies, 1);
}
