use std::str::FromStr;

use tuilane_plugin_api::{ViewDescriptor, ViewId};

#[test]
fn view_ids_accept_stable_lowercase_identifiers() {
    for value in ["overview", "tasks.list", "plugin-calendar_month", "v2"] {
        assert_eq!(ViewId::from_str(value).unwrap().as_str(), value);
    }
}

#[test]
fn view_ids_reject_empty_or_ambiguous_identifiers() {
    for value in ["", "Overview", "two words", "ümlaut", "a/b"] {
        assert!(ViewId::from_str(value).is_err(), "accepted {value:?}");
    }
}

#[test]
fn descriptor_exposes_host_neutral_metadata() {
    let descriptor = ViewDescriptor::new(
        ViewId::new("overview").unwrap(),
        "Overview",
        "Workspace summary",
    );

    assert_eq!(descriptor.id().as_str(), "overview");
    assert_eq!(descriptor.title(), "Overview");
    assert_eq!(descriptor.description(), "Workspace summary");
}
