use std::num::NonZeroUsize;
use lru::LruCache;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, EventEmitter, IntoElement, Render, Window, div};

use crate::focus::NavFocusable;
use crate::lifecycle::ScreenLifecycle;
use crate::route::{NavEntry, RouteConfig};

/// Single-slot replaceable panel navigator with an LRU cache (`SlotNavigator`).
///
/// Designed for messenger chat rooms, editor tabs, and detail views:
/// - Maintains one active screen and a bounded LRU cache of recently visited screens.
/// - Preserves state, scroll position, input drafts, and background tasks for cached screens.
/// - **Lifecycle Management**: Suspends (`ScreenLifecycle::Paused`) cached screens and detaches
///   listeners so background socket events in inactive chats cannot trigger parent router handlers.
/// - When cache exceeds `capacity`, the least recently used `NavEntry` is evicted, transitioned to
///   `ScreenLifecycle::Destroyed`, and dropped via RAII.
pub struct SlotNavigator<C: RouteConfig, V: Render + 'static> {
    active: Option<NavEntry<C, V>>,
    cache: Option<LruCache<C, NavEntry<C, V>>>,
    capacity: usize,
}

impl<C: RouteConfig, V: Render + 'static> SlotNavigator<C, V> {
    /// Creates a new slot navigator with the specified LRU cache capacity.
    pub fn new(capacity: usize) -> Self {
        let cache = NonZeroUsize::new(capacity).map(LruCache::new);
        Self { active: None, cache, capacity }
    }

    /// Activates a route configuration.
    ///
    /// - If `config` is already active, returns the existing entity.
    /// - If `config` exists in the LRU cache, promotes it to active (retaining state & scroll).
    /// - If `config` is not in cache, instantiates a new entity via `factory` and subscribes to events.
    /// - Moves previous active screen into LRU cache (pausing it and detaching output listeners).
    /// - Triggers `cx.notify()`.
    pub fn activate<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> &Entity<V>
    where
        V: EventEmitter<O>,
    {
        // Check if already active
        if self.active.as_ref().map(|e| &e.config) == Some(&config) {
            return &self.active.as_ref().unwrap().entity;
        }

        // Check if present in LRU cache (O(1) lookup & removal)
        if let Some(mut promoted) = self.cache.as_mut().and_then(|c| c.pop(&config)) {
            self.shelve_active(cx);
            promoted.set_lifecycle(ScreenLifecycle::Resumed);

            // Re-subscribe when promoted to active
            let sub = cx.subscribe(&promoted.entity, move |parent, child, event: &O, cx| {
                on_output(parent, child, event, cx);
            });
            promoted.set_subscription(Some(sub));

            self.active = Some(promoted);
            cx.notify();
            return &self.active.as_ref().unwrap().entity;
        }

        // Not in cache: shelve old active and create fresh entity
        self.shelve_active(cx);

        let entity = cx.new(|cx| factory(&config, cx));
        let subscription = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
            on_output(parent, child, event, cx);
        });

        self.active = Some(NavEntry::new(config, entity, Some(subscription)));
        cx.notify();
        &self.active.as_ref().unwrap().entity
    }

    /// Activates a route configuration with access to `Window`.
    pub fn activate_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Window, &mut Context<P>) + 'static,
    ) -> &Entity<V>
    where
        V: EventEmitter<O>,
    {
        if self.active.as_ref().map(|e| &e.config) == Some(&config) {
            return &self.active.as_ref().unwrap().entity;
        }

        if let Some(mut promoted) = self.cache.as_mut().and_then(|c| c.pop(&config)) {
            self.shelve_active(cx);
            promoted.set_lifecycle(ScreenLifecycle::Resumed);

            let sub = cx.subscribe_in(&promoted.entity, window, move |parent, child, event: &O, window, cx| {
                on_output(parent, child.clone(), event, window, cx);
            });
            promoted.set_subscription(Some(sub));

            self.active = Some(promoted);
            cx.notify();
            return &self.active.as_ref().unwrap().entity;
        }

        self.shelve_active(cx);

        let entity = cx.new(|cx| factory(&config, window, cx));
        let subscription =
            cx.subscribe_in(&entity, window, move |parent, child, event: &O, window, cx| {
                on_output(parent, child.clone(), event, window, cx);
            });

        self.active = Some(NavEntry::new(config, entity, Some(subscription)));
        cx.notify();
        &self.active.as_ref().unwrap().entity
    }

    /// Activates a route configuration with access to `Window` and transfers keyboard focus.
    pub fn activate_and_focus_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Window, &mut Context<P>) + 'static,
    ) -> &Entity<V>
    where
        V: EventEmitter<O> + NavFocusable,
    {
        self.activate_in(config, window, cx, factory, on_output);
        self.focus_active(window, cx);
        &self.active.as_ref().unwrap().entity
    }

    /// Activates a route configuration without subscribing to events.
    pub fn activate_simple<P: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
    ) -> &Entity<V> {
        if self.active.as_ref().map(|e| &e.config) == Some(&config) {
            return &self.active.as_ref().unwrap().entity;
        }

        if let Some(mut promoted) = self.cache.as_mut().and_then(|c| c.pop(&config)) {
            self.shelve_active(cx);
            promoted.set_lifecycle(ScreenLifecycle::Resumed);
            self.active = Some(promoted);
            cx.notify();
            return &self.active.as_ref().unwrap().entity;
        }

        self.shelve_active(cx);

        let entity = cx.new(|cx| factory(&config, cx));
        self.active = Some(NavEntry::new(config, entity, None));
        cx.notify();
        &self.active.as_ref().unwrap().entity
    }

    /// Activates a route and transfers focus to it.
    pub fn activate_and_focus<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> &Entity<V>
    where
        V: EventEmitter<O> + NavFocusable,
    {
        self.activate(config, cx, factory, on_output);
        self.focus_active(window, cx);
        &self.active.as_ref().unwrap().entity
    }

    /// Shelves the current active entry into the LRU cache.
    ///
    /// Transitions the shelved screen to `ScreenLifecycle::Paused` and detaches
    /// its active output subscription to prevent zombie events from leaking into the parent.
    fn shelve_active<P: 'static>(&mut self, _cx: &mut Context<P>) {
        if let Some(mut old_active) = self.active.take()
            && let Some(cache) = self.cache.as_mut()
        {
            old_active.set_lifecycle(ScreenLifecycle::Paused);
            old_active.detach_subscription();

            // If the cache is full, the evicted entry is permanently destroyed
            if cache.len() >= self.capacity
                && let Some((_k, mut evicted)) = cache.pop_lru()
            {
                evicted.set_lifecycle(ScreenLifecycle::Destroyed);
            }

            cache.put(old_active.config.clone(), old_active);
        }
    }

    /// Clears the active slot (closing the current screen) without clearing the LRU cache.
    pub fn clear<P: 'static>(&mut self, cx: &mut Context<P>) {
        if let Some(mut active) = self.active.take() {
            active.set_lifecycle(ScreenLifecycle::Destroyed);
            cx.notify();
        }
    }

    /// Clears both the active screen and the entire LRU cache.
    pub fn clear_all<P: 'static>(&mut self, cx: &mut Context<P>) {
        if let Some(mut active) = self.active.take() {
            active.set_lifecycle(ScreenLifecycle::Destroyed);
        }
        if let Some(cache) = self.cache.as_mut() {
            while let Some((_k, mut entry)) = cache.pop_lru() {
                entry.set_lifecycle(ScreenLifecycle::Destroyed);
            }
        }
        cx.notify();
    }

    /// Returns the currently active route configuration.
    pub fn active_config(&self) -> Option<&C> {
        self.active.as_ref().map(|e| &e.config)
    }

    /// Returns the currently active entity.
    pub fn active_entity(&self) -> Option<&Entity<V>> {
        self.active.as_ref().map(|e| &e.entity)
    }

    /// Returns the capacity of the LRU cache.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of cached screens in the LRU cache.
    pub fn cache_len(&self) -> usize {
        self.cache.as_ref().map_or(0, |c| c.len())
    }

    /// Returns the configurations of all cached screens.
    pub fn cached_configs(&self) -> Vec<C> {
        self.cache.as_ref().map_or_else(Vec::new, |c| c.iter().map(|(k, _)| k.clone()).collect())
    }

    /// Returns `true` if the given config is active or in the cache.
    pub fn contains(&self, config: &C) -> bool {
        self.active_config() == Some(config) || self.cache.as_ref().is_some_and(|c| c.contains(config))
    }

    /// Requests focus on the currently active screen.
    pub fn focus_active(&self, window: &mut Window, cx: &mut App) -> bool
    where
        V: NavFocusable,
    {
        if let Some(active) = &self.active
            && let Some(handle) = active.entity.read(cx).focus_handle(cx)
        {
            window.focus(&handle, cx);
            return true;
        }
        false
    }

    /// Renders the active screen, or an empty container if no screen is active.
    pub fn render(&self) -> AnyElement {
        if let Some(active) = &self.active {
            active.entity.clone().into_any_element()
        } else {
            div().into_any_element()
        }
    }

    /// Renders the active screen, or executes the `fallback` closure if no screen is active.
    pub fn render_or_else<E: IntoElement>(&self, fallback: impl FnOnce() -> E) -> AnyElement {
        if let Some(active) = &self.active {
            active.entity.clone().into_any_element()
        } else {
            fallback().into_any_element()
        }
    }
}
