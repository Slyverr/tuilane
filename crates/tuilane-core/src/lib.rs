//! Application kernel primitives that do not depend on a terminal renderer or storage backend.

use tuilane_plugin_api::{ViewDescriptor, ViewId};

/// Ordered host registry for views and their implementations.
#[derive(Debug, Default)]
pub struct ViewRegistry<T> {
    views: Vec<RegisteredView<T>>,
}

/// A registered view and the host-neutral metadata used to identify it.
#[derive(Debug)]
pub struct RegisteredView<T> {
    descriptor: ViewDescriptor,
    value: T,
}

/// Failure returned when a view identifier is already registered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateViewId(pub ViewId);

impl<T> ViewRegistry<T> {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self { views: Vec::new() }
    }

    /// Registers a view, preserving registration order.
    pub fn register(
        &mut self,
        descriptor: ViewDescriptor,
        value: T,
    ) -> Result<(), DuplicateViewId> {
        if self.contains(descriptor.id()) {
            return Err(DuplicateViewId(descriptor.id().clone()));
        }

        self.views.push(RegisteredView { descriptor, value });
        Ok(())
    }

    /// Returns the number of registered views.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Returns whether the registry has no views.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Returns whether the identifier is registered.
    pub fn contains(&self, id: &ViewId) -> bool {
        self.views.iter().any(|view| view.descriptor.id() == id)
    }

    /// Returns an immutable registered view by its position.
    pub fn get(&self, index: usize) -> Option<&RegisteredView<T>> {
        self.views.get(index)
    }

    /// Returns a mutable registered view by its position.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut RegisteredView<T>> {
        self.views.get_mut(index)
    }

    /// Removes a view by identifier and returns its implementation.
    pub fn remove(&mut self, id: &ViewId) -> Option<RegisteredView<T>> {
        self.views
            .iter()
            .position(|view| view.descriptor.id() == id)
            .map(|index| self.views.remove(index))
    }

    /// Iterates over registered views in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &RegisteredView<T>> {
        self.views.iter()
    }
}

impl<T> RegisteredView<T> {
    /// Returns the view's stable metadata.
    pub fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    /// Returns the registered implementation.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns the registered implementation mutably.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consumes the entry and returns its implementation.
    pub fn into_value(self) -> T {
        self.value
    }
}
