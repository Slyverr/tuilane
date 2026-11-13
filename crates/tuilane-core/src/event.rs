use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use tuilane_plugin_api::Event;

type ErasedHandler = Box<dyn Fn(&dyn Any)>;

struct Subscriber {
    handler: ErasedHandler,
}

/// Keeps a typed event subscription active until the handle is dropped.
pub struct EventSubscription {
    _subscriber: Rc<Subscriber>,
}

/// Synchronous, typed event channel shared by the application host and features.
#[derive(Clone, Default)]
pub struct EventBus {
    subscribers: Rc<RefCell<HashMap<TypeId, Vec<Weak<Subscriber>>>>>,
}

impl EventBus {
    /// Creates an empty event channel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to one event type. Dropping the returned handle unsubscribes.
    pub fn subscribe<E: Event>(&self, handler: impl Fn(&E) + 'static) -> EventSubscription {
        let subscriber = Rc::new(Subscriber {
            handler: Box::new(move |event| {
                if let Some(event) = event.downcast_ref::<E>() {
                    handler(event);
                }
            }),
        });
        self.subscribers
            .borrow_mut()
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Rc::downgrade(&subscriber));
        EventSubscription {
            _subscriber: subscriber,
        }
    }

    /// Publishes an event to the active subscribers of its concrete type.
    pub fn publish<E: Event>(&self, event: E) {
        let subscribers = {
            let mut registry = self.subscribers.borrow_mut();
            let Some(handlers) = registry.get_mut(&TypeId::of::<E>()) else {
                return;
            };
            handlers.retain(|subscriber| subscriber.strong_count() > 0);
            handlers
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>()
        };

        let event = &event as &dyn Any;
        for subscriber in subscribers {
            (subscriber.handler)(event);
        }
    }
}
