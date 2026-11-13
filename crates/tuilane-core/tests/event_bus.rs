use std::{cell::Cell, rc::Rc};

use tuilane_core::EventBus;
use tuilane_plugin_api::Event;

struct Refresh;

impl Event for Refresh {
    const NAME: &'static str = "test.refresh@1";
}

struct Other;

impl Event for Other {
    const NAME: &'static str = "test.other@1";
}

#[test]
fn publishes_only_to_subscribers_of_the_matching_event_type() {
    let events = EventBus::new();
    let refresh_count = Rc::new(Cell::new(0));
    let other_count = Rc::new(Cell::new(0));

    let refresh_target = Rc::clone(&refresh_count);
    let _refresh_subscription = events.subscribe(move |_: &Refresh| {
        refresh_target.set(refresh_target.get() + 1);
    });
    let other_target = Rc::clone(&other_count);
    let _other_subscription = events.subscribe(move |_: &Other| {
        other_target.set(other_target.get() + 1);
    });

    events.publish(Refresh);

    assert_eq!(refresh_count.get(), 1);
    assert_eq!(other_count.get(), 0);
}

#[test]
fn dropping_a_subscription_stops_future_notifications() {
    let events = EventBus::new();
    let count = Rc::new(Cell::new(0));
    let target = Rc::clone(&count);
    let subscription = events.subscribe(move |_: &Refresh| {
        target.set(target.get() + 1);
    });

    events.publish(Refresh);
    drop(subscription);
    events.publish(Refresh);

    assert_eq!(count.get(), 1);
}

#[test]
fn cloned_buses_share_subscriptions() {
    let events = EventBus::new();
    let writer = events.clone();
    let count = Rc::new(Cell::new(0));
    let target = Rc::clone(&count);
    let _subscription = events.subscribe(move |_: &Refresh| {
        target.set(target.get() + 1);
    });

    writer.publish(Refresh);

    assert_eq!(count.get(), 1);
}

#[test]
fn subscribers_can_republish_their_event_synchronously() {
    let events = EventBus::new();
    let nested_publisher = events.clone();
    let count = Rc::new(Cell::new(0));
    let target = Rc::clone(&count);
    let _subscription = events.subscribe(move |_: &Refresh| {
        let current = target.get() + 1;
        target.set(current);
        if current == 1 {
            nested_publisher.publish(Refresh);
        }
    });

    events.publish(Refresh);

    assert_eq!(count.get(), 2);
}
