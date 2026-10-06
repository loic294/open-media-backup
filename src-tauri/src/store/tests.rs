use super::*;
use crate::domain::{EntityKind, Space};

fn space(id: &str, name: &str) -> Space {
    Space {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    }
}

#[test]
fn copy_policy_migration_preserves_per_space_maximum_and_syncs_once() {
    use crate::domain::Project;
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copy-policy.sqlite");
    let conn = Connection::open(&path).unwrap();
    schema::migrate(&conn).unwrap();
    for (kind, id, data) in [
        ("space", "a", json!({"id":"a"})),
        ("space", "b", json!({"id":"b"})),
        ("space", "empty", json!({"id":"empty"})),
        (
            "space",
            "explicit",
            json!({"id":"explicit","final_copies_required":1}),
        ),
        (
            "project",
            "a1",
            json!({"id":"a1","space_id":"a","final_copies_required":1}),
        ),
        (
            "project",
            "a2",
            json!({"id":"a2","space_id":"a","final_copies_required":5,"archived":true}),
        ),
        (
            "project",
            "b1",
            json!({"id":"b1","space_id":"b","final_copies_required":3}),
        ),
        (
            "project",
            "e1",
            json!({"id":"e1","space_id":"explicit","final_copies_required":7}),
        ),
        (
            "project",
            "deleted",
            json!({"id":"deleted","space_id":"b","final_copies_required":9}),
        ),
    ] {
        conn.execute(
            "INSERT INTO entities(kind,id,data,deleted) VALUES (?1,?2,?3,?4)",
            rusqlite::params![kind, id, data.to_string(), id == "deleted"],
        )
        .unwrap();
    }
    let store = Store::from_connection(conn).unwrap();
    for (id, expected) in [("a", 5), ("b", 3), ("empty", 2), ("explicit", 1)] {
        assert_eq!(
            store
                .get::<Space>(id)
                .unwrap()
                .unwrap()
                .final_copies_required,
            expected
        );
    }
    let project = store.get::<Project>("a2").unwrap().unwrap();
    assert!(serde_json::to_value(project)
        .unwrap()
        .get("final_copies_required")
        .is_none());
    let ops = store.ops_since(&VersionVector::new(), 1000).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(ops
        .iter()
        .all(|op| op.kind == "space" && op.field == "final_copies_required"));
    let peer = Store::open_in_memory().unwrap();
    peer.apply_remote(&ops).unwrap();
    assert_eq!(
        peer.get::<Space>("a")
            .unwrap()
            .unwrap()
            .final_copies_required,
        5
    );
    store.migrate_copy_policy().unwrap();
    assert_eq!(store.ops_since(&VersionVector::new(), 1000).unwrap(), ops);
    let mut changed = store.get::<Space>("a").unwrap().unwrap();
    changed.final_copies_required = 2;
    store.put(&changed).unwrap();
    peer.apply_remote(
        &store
            .ops_since(&peer.version_vector().unwrap(), 1000)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(peer.get::<Space>("a").unwrap(), Some(changed));
    store.migrate_copy_policy().unwrap();
    assert_eq!(
        store
            .get::<Space>("a")
            .unwrap()
            .unwrap()
            .final_copies_required,
        2
    );
    let before_restart = store.ops_since(&VersionVector::new(), 1000).unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened
            .get::<Space>("a")
            .unwrap()
            .unwrap()
            .final_copies_required,
        2
    );
    assert_eq!(
        reopened
            .get::<Space>("b")
            .unwrap()
            .unwrap()
            .final_copies_required,
        3
    );
    assert_eq!(
        reopened.ops_since(&VersionVector::new(), 1000).unwrap(),
        before_restart
    );
}

#[test]
fn invalid_space_copy_policy_is_rejected_by_sync_without_partial_writes() {
    let store = Store::open_in_memory().unwrap();
    for value in [
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!(4294967296_u64),
        serde_json::json!("2"),
    ] {
        let op = Op {
            hlc: store.clock.now().encode(),
            origin: "peer".into(),
            kind: "space".into(),
            entity_id: "s".into(),
            field: "final_copies_required".into(),
            value,
        };
        assert!(store.apply_remote(&[op]).is_err());
        assert!(store.get::<Space>("s").unwrap().is_none());
        assert!(store
            .ops_since(&VersionVector::new(), 1000)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn hash_server_credentials_are_local_but_destination_mapping_syncs() {
    use crate::domain::{Destination, RemoteHash};
    let store = Store::open_in_memory().unwrap();
    let server = HashServer {
        id: "nas".into(),
        name: "NAS".into(),
        address: "nas:47822".into(),
        token: "private-token".into(),
        ..Default::default()
    };
    store.save_hash_server(&server).unwrap();
    assert_eq!(store.hash_servers().unwrap(), vec![server]);
    assert!(store
        .ops_since(&VersionVector::new(), 1000)
        .unwrap()
        .is_empty());
    let destination = Destination {
        id: "destination".into(),
        remote_hash: Some(RemoteHash {
            server_id: "nas".into(),
            root: "root".into(),
            enabled: true,
        }),
        ..Default::default()
    };
    store.put(&destination).unwrap();
    let ops = store.ops_since(&VersionVector::new(), 1000).unwrap();
    let serialized = serde_json::to_string(&ops).unwrap();
    assert!(!serialized.contains("private-token"));
    assert!(!serialized.contains("nas:47822"));
    let other = Store::open_in_memory().unwrap();
    other.apply_remote(&ops).unwrap();
    assert_eq!(
        other.get::<Destination>("destination").unwrap(),
        Some(destination)
    );
    assert!(other.hash_servers().unwrap().is_empty());
    store.remove_hash_server("nas").unwrap();
    assert!(store.hash_servers().unwrap().is_empty());
}
#[test]
fn put_get_list_delete() {
    let store = Store::open_in_memory().unwrap();
    store.put(&space("s1", "Travel")).unwrap();
    assert_eq!(store.get::<Space>("s1").unwrap().unwrap().name, "Travel");
    assert_eq!(store.list::<Space>().unwrap().len(), 1);
    store.delete(EntityKind::Space, "s1").unwrap();
    assert!(store.get::<Space>("s1").unwrap().is_none());
    store.put(&space("s1", "Back")).unwrap();
    assert_eq!(store.get::<Space>("s1").unwrap().unwrap().name, "Back");
}

#[test]
fn only_changed_fields_produce_ops() {
    let store = Store::open_in_memory().unwrap();
    let mut s = space("s1", "Travel");
    store.put(&s).unwrap();
    let before = store.ops_since(&VersionVector::new(), 1000).unwrap().len();
    s.name = "Home".into();
    store.put(&s).unwrap();
    assert_eq!(
        store.ops_since(&VersionVector::new(), 1000).unwrap().len(),
        before + 1
    );
}

#[test]
fn list_by_field() {
    let store = Store::open_in_memory().unwrap();
    store.put(&space("a", "One")).unwrap();
    store.put(&space("b", "Two")).unwrap();
    let found: Vec<Space> = store.list_by("name", "Two").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "b");
}

#[test]
fn peers_converge_last_writer_wins() {
    let a = Store::open_in_memory().unwrap();
    let b = Store::open_in_memory().unwrap();
    a.put(&space("s1", "Travel")).unwrap();
    b.apply_remote(&a.ops_since(&b.version_vector().unwrap(), 1000).unwrap())
        .unwrap();

    let mut on_a = a.get::<Space>("s1").unwrap().unwrap();
    on_a.name = "From A".into();
    a.put(&on_a).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut on_b = b.get::<Space>("s1").unwrap().unwrap();
    on_b.icon = "home".into();
    on_b.name = "From B".into();
    b.put(&on_b).unwrap();

    a.apply_remote(&b.ops_since(&a.version_vector().unwrap(), 1000).unwrap())
        .unwrap();
    b.apply_remote(&a.ops_since(&b.version_vector().unwrap(), 1000).unwrap())
        .unwrap();
    let (fa, fb) = (
        a.get::<Space>("s1").unwrap().unwrap(),
        b.get::<Space>("s1").unwrap().unwrap(),
    );
    assert_eq!(fa, fb);
    assert_eq!(fa.name, "From B");
    assert_eq!(fa.icon, "home");
}

#[test]
fn applying_same_ops_twice_is_idempotent() {
    let a = Store::open_in_memory().unwrap();
    let b = Store::open_in_memory().unwrap();
    a.put(&space("s1", "Travel")).unwrap();
    let ops = a.ops_since(&VersionVector::new(), 1000).unwrap();
    assert!(b.apply_remote(&ops).unwrap() > 0);
    assert_eq!(b.apply_remote(&ops).unwrap(), 0);
}

#[test]
fn deleting_device_persists_detachment_and_syncs_without_losing_tasks_or_catalog() {
    use crate::domain::{Destination, Device, DeviceMapping, FileCopy, FileRecord, Flow, Source};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("devices.sqlite");
    let store = Store::open(&path).unwrap();
    let peer = Store::open_in_memory().unwrap();
    for id in ["removed", "kept"] {
        store
            .put(&Device {
                id: id.into(),
                name: id.into(),
                ..Default::default()
            })
            .unwrap();
    }
    let sources = ["space-a", "space-b"].map(|space_id| Source {
        id: format!("source-{space_id}"),
        space_id: space_id.into(),
        device_id: "removed".into(),
        task_name: "Import".into(),
        backup_name: "Camera".into(),
        path_template: "DCIM".into(),
        offer_wipe: true,
        ..Default::default()
    });
    store.put_all(&sources).unwrap();
    let destination = Destination {
        id: "destination".into(),
        space_id: "space-b".into(),
        device_id: "removed".into(),
        task_name: "Archive".into(),
        path_template: "backup".into(),
        ..Default::default()
    };
    store.put(&destination).unwrap();
    let unrelated = Source {
        id: "unrelated".into(),
        device_id: "kept".into(),
        ..Default::default()
    };
    store.put(&unrelated).unwrap();
    for computer_id in ["computer-a", "computer-b"] {
        store
            .put(&DeviceMapping {
                id: format!("removed@{computer_id}"),
                device_id: "removed".into(),
                computer_id: computer_id.into(),
                root_path: "/card".into(),
            })
            .unwrap();
    }
    let flow = Flow {
        id: "flow".into(),
        space_id: "space-b".into(),
        source_id: sources[1].id.clone(),
        destination_id: destination.id.clone(),
    };
    store.put(&flow).unwrap();
    let record = FileRecord {
        id: "file".into(),
        origin_device_id: "removed".into(),
        ..Default::default()
    };
    let copy = FileCopy {
        id: "copy".into(),
        file_id: "file".into(),
        device_id: "removed".into(),
        ..Default::default()
    };
    store.put(&record).unwrap();
    store.put(&copy).unwrap();
    peer.apply_remote(
        &store
            .ops_since(&peer.version_vector().unwrap(), 1000)
            .unwrap(),
    )
    .unwrap();
    let baseline = store.version_vector().unwrap();
    let mut notifications = store.subscribe();
    store.delete_device("removed").unwrap();
    let event = notifications.try_recv().unwrap();
    assert!(event.kinds.contains(&"source".into()));
    assert!(event.kinds.contains(&"destination".into()));
    assert!(event.kinds.contains(&"device_mapping".into()));
    assert!(event.kinds.contains(&"device".into()));
    assert!(notifications.try_recv().is_err());
    let ops = store.ops_since(&baseline, 1000).unwrap();
    assert_eq!(ops.len(), 6);
    assert_eq!(ops.iter().filter(|op| op.field == "device_id").count(), 3);
    peer.apply_remote(&ops).unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    for s in [&reopened, &peer] {
        assert!(s.get::<Device>("removed").unwrap().is_none());
        assert!(s.get::<Device>("kept").unwrap().is_some());
        assert!(s.list::<DeviceMapping>().unwrap().is_empty());
        for source in &sources {
            let mut expected = source.clone();
            expected.device_id.clear();
            assert_eq!(s.get::<Source>(&source.id).unwrap().unwrap(), expected);
        }
        let mut expected = destination.clone();
        expected.device_id.clear();
        assert_eq!(
            s.get::<Destination>("destination").unwrap().unwrap(),
            expected
        );
        assert_eq!(s.get::<Source>("unrelated").unwrap().unwrap(), unrelated);
        assert_eq!(s.get::<Flow>("flow").unwrap().unwrap(), flow);
        assert_eq!(s.get::<FileRecord>("file").unwrap().unwrap(), record);
        assert_eq!(s.get::<FileCopy>("copy").unwrap().unwrap(), copy);
    }
}

#[test]
fn device_deletion_rolls_back_every_change_on_failure() {
    use crate::domain::{Device, DeviceMapping, Source};
    let store = Store::open_in_memory().unwrap();
    let device = Device {
        id: "device".into(),
        ..Default::default()
    };
    let source = Source {
        id: "source".into(),
        device_id: device.id.clone(),
        ..Default::default()
    };
    let mapping = DeviceMapping {
        id: "mapping".into(),
        device_id: device.id.clone(),
        ..Default::default()
    };
    store.put(&device).unwrap();
    store.put(&source).unwrap();
    store.put(&mapping).unwrap();
    let baseline = store.version_vector().unwrap();
    let mut notifications = store.subscribe();
    store
        .conn
        .lock()
        .execute_batch(
            "CREATE TEMP TRIGGER reject_device_delete BEFORE INSERT ON entities
         WHEN NEW.kind = 'device' AND NEW.deleted = 1
         BEGIN SELECT RAISE(ABORT, 'simulated deletion failure'); END;",
        )
        .unwrap();
    assert!(store.delete_device("device").is_err());
    assert_eq!(store.get::<Source>("source").unwrap().unwrap(), source);
    assert_eq!(
        store.get::<DeviceMapping>("mapping").unwrap().unwrap(),
        mapping
    );
    assert_eq!(store.get::<Device>("device").unwrap().unwrap(), device);
    assert!(store.ops_since(&baseline, 1000).unwrap().is_empty());
    assert!(notifications.try_recv().is_err());
    assert!(store.delete_device("").is_err());
}

#[test]
fn settings_and_peers() {
    let store = Store::open_in_memory().unwrap();
    assert!(!store.computer_id().is_empty());
    store.set_setting("theme", "dark").unwrap();
    assert_eq!(store.setting("theme").unwrap().as_deref(), Some("dark"));
    store
        .save_peer(&Peer {
            id: "p".into(),
            name: "NAS".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(store.peers().unwrap().len(), 1);
    store.remove_peer("p").unwrap();
    assert!(store.peers().unwrap().is_empty());
}

#[test]
fn task_and_backup_names_survive_reopen_and_sync_as_independent_fields() {
    use crate::domain::{Destination, Device, Source};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("names.sqlite");
    let mut source = Source {
        id: "source".into(),
        device_id: "device".into(),
        task_name: "Import".into(),
        backup_name: "Camera".into(),
        ..Default::default()
    };
    let destination = Destination {
        id: "destination".into(),
        device_id: "device".into(),
        task_name: "Archive".into(),
        ..Default::default()
    };
    let device = Device {
        id: "device".into(),
        name: "Shared physical name".into(),
        ..Default::default()
    };
    {
        let store = Store::open(&path).unwrap();
        store.put(&source).unwrap();
        store.put(&destination).unwrap();
        store.put(&device).unwrap();
    }
    let a = Store::open(&path).unwrap();
    let b = Store::open_in_memory().unwrap();
    assert_eq!(a.get::<Source>(&source.id).unwrap().unwrap(), source);
    assert_eq!(
        a.get::<Destination>(&destination.id).unwrap().unwrap(),
        destination
    );
    b.apply_remote(&a.ops_since(&b.version_vector().unwrap(), 1000).unwrap())
        .unwrap();
    let baseline = a.version_vector().unwrap();
    source.task_name = "New import label".into();
    a.put(&source).unwrap();
    let changes = a.ops_since(&baseline, 1000).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "task_name");
    let mut on_b = b.get::<Source>(&source.id).unwrap().unwrap();
    on_b.backup_name = "New backup name".into();
    b.put(&on_b).unwrap();
    let mut destination_on_b = destination.clone();
    destination_on_b.task_name = "New archive label".into();
    b.put(&destination_on_b).unwrap();
    a.apply_remote(&b.ops_since(&a.version_vector().unwrap(), 1000).unwrap())
        .unwrap();
    b.apply_remote(&a.ops_since(&b.version_vector().unwrap(), 1000).unwrap())
        .unwrap();
    let merged = a.get::<Source>(&source.id).unwrap().unwrap();
    assert_eq!(merged.task_name, "New import label");
    assert_eq!(merged.backup_name, "New backup name");
    assert_eq!(b.get::<Source>(&source.id).unwrap().unwrap(), merged);
    for store in [&a, &b] {
        assert_eq!(
            store.get::<Destination>(&destination.id).unwrap().unwrap(),
            destination_on_b
        );
        assert_eq!(store.get::<Device>(&device.id).unwrap().unwrap(), device);
    }
}
