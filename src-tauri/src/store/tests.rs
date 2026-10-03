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
