use tuilane_core::ViewRegistry;
use tuilane_plugin_api::{ViewDescriptor, ViewId};

fn descriptor(id: &str, title: &str) -> ViewDescriptor {
    ViewDescriptor::new(ViewId::new(id).unwrap(), title, "test view")
}

#[test]
fn registry_preserves_order_and_removes_by_id() {
    let mut registry = ViewRegistry::new();
    registry
        .register(descriptor("overview", "Overview"), 1)
        .unwrap();
    registry
        .register(descriptor("activity", "Activity"), 2)
        .unwrap();

    assert_eq!(
        registry
            .iter()
            .map(|entry| *entry.value())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        registry
            .remove(&ViewId::new("overview").unwrap())
            .unwrap()
            .into_value(),
        1
    );
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get(0).unwrap().descriptor().title(), "Activity");
}

#[test]
fn registry_rejects_duplicate_ids() {
    let mut registry = ViewRegistry::new();
    registry
        .register(descriptor("overview", "Overview"), ())
        .unwrap();

    assert!(
        registry
            .register(descriptor("overview", "Another overview"), ())
            .is_err()
    );
    assert_eq!(registry.len(), 1);
}
