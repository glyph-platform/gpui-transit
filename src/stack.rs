use tokio::sync::oneshot;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, EventEmitter, IntoElement, Render, Window, div};

use crate::focus::NavFocusable;
use crate::lifecycle::ScreenLifecycle;
use crate::result::{ResultRouteConfig, ResultSender};
use crate::route::{NavEntry, RouteConfig};
use crate::tree::{NavigationFlow, NavigationNode, NavigationNodeId};

/// Stack-based hierarchical navigator (`StackNavigator`).
///
/// Features:
/// - Maintains a history stack of screen entries (`NavEntry<C, V>`).
/// - **Lifecycle Management**: Automatically updates screens to `Resumed`, `Paused`, and `Destroyed`.
/// - **Deep Focus Restoration**: Automatically records the active focus handle before pushing,
///   and restores focus upon popping.
/// - **Typed Results (`push_for_result`)**: Open dialogs/pickers and await results via Rust async channels.
/// - **Navigation Guards**: Intercepts transitions if unsaved data needs confirmation.
/// - **Navigation Tree Integration**: Implements `NavigationNode` for automatic Back bubbling.
pub struct StackNavigator<C: RouteConfig, V: Render + 'static> {
    entries: Vec<NavEntry<C, V>>,
    node_id: NavigationNodeId,
}

impl<C: RouteConfig, V: Render + 'static> Default for StackNavigator<C, V> {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl<C: RouteConfig, V: Render + 'static> StackNavigator<C, V> {
    /// Creates an empty stack navigator.
    pub fn new_empty() -> Self {
        Self {
            entries: Vec::new(),
            node_id: NavigationNodeId::next(),
        }
    }

    /// Initializes a stack navigator with a root route.
    pub fn new<P: 'static, O: 'static>(
        root_config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> Self
    where
        V: EventEmitter<O>,
    {
        let mut navigator = Self::new_empty();
        navigator.push(root_config, cx, factory, on_output);
        navigator
    }

    /// Initializes a stack navigator with a root route without output events.
    pub fn new_simple<P: 'static>(
        root_config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
    ) -> Self {
        let mut navigator = Self::new_empty();
        navigator.push_simple(root_config, cx, factory);
        navigator
    }

    /// Pushes a new configuration onto the stack.
    ///
    /// - Safeguards against duplicate top entries (`current_config() == Some(&config)`).
    /// - Pauses the previous active screen.
    /// - Instantiates `Entity<V>` via the provided factory in `Resumed` state.
    /// - Automatically subscribes the parent to the child's `EventEmitter<O>` events.
    /// - Triggers `cx.notify()`.
    ///
    /// Returns `true` if pushed, `false` if rejected as a duplicate.
    pub fn push<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> bool
    where
        V: EventEmitter<O>,
    {
        if self.current_config() == Some(&config) {
            return false;
        }

        // Pause previous active screen
        if let Some(prev) = self.entries.last_mut() {
            prev.set_lifecycle(ScreenLifecycle::Paused);
        }

        let entity = cx.new(|cx| factory(&config, cx));
        let subscription = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
            on_output(parent, child, event, cx);
        });

        self.entries.push(NavEntry::new(config, entity, Some(subscription)));
        cx.notify();
        true
    }

    /// Pushes a new configuration without subscribing to events.
    pub fn push_simple<P: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
    ) -> bool {
        if self.current_config() == Some(&config) {
            return false;
        }

        if let Some(prev) = self.entries.last_mut() {
            prev.set_lifecycle(ScreenLifecycle::Paused);
        }

        let entity = cx.new(|cx| factory(&config, cx));
        self.entries.push(NavEntry::new(config, entity, None));
        cx.notify();
        true
    }

    /// Pushes a new configuration and transfers focus to it, recording current focus for restoration.
    pub fn push_and_focus<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> bool
    where
        V: EventEmitter<O> + NavFocusable,
    {
        // Record focus of current screen
        let current_focus = window.focused(cx);
        if let Some(prev) = self.entries.last_mut() {
            prev.save_focus(current_focus);
        }

        if self.push(config, cx, factory, on_output) {
            self.focus_active(window, cx);
            true
        } else {
            false
        }
    }

    /// Pushes a configuration with access to `Window`.
    pub fn push_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Window, &mut Context<P>) + 'static,
    ) -> bool
    where
        V: EventEmitter<O>,
    {
        if self.current_config() == Some(&config) {
            return false;
        }

        if let Some(prev) = self.entries.last_mut() {
            prev.set_lifecycle(ScreenLifecycle::Paused);
        }

        let entity = cx.new(|cx| factory(&config, window, cx));
        let subscription =
            cx.subscribe_in(&entity, window, move |parent, child, event: &O, window, cx| {
                on_output(parent, child.clone(), event, window, cx);
            });

        self.entries.push(NavEntry::new(config, entity, Some(subscription)));
        cx.notify();
        true
    }

    /// Pushes a new configuration with `Window` access and transfers focus.
    pub fn push_and_focus_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Window, &mut Context<P>) + 'static,
    ) -> bool
    where
        V: EventEmitter<O> + NavFocusable,
    {
        let current_focus = window.focused(cx);
        if let Some(prev) = self.entries.last_mut() {
            prev.save_focus(current_focus);
        }

        if self.push_in(config, window, cx, factory, on_output) {
            self.focus_active(window, cx);
            true
        } else {
            false
        }
    }

    /// Pushes a route that produces a typed result, returning an asynchronous receiver.
    ///
    /// The factory receives a `ResultSender<C::Result>` to deliver the return value.
    /// If the screen is popped without sending a result, the receiver yields `Err(NavigationResultError::Cancelled)`.
    pub fn push_for_result<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, ResultSender<C::Result>, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) -> oneshot::Receiver<C::Result>
    where
        C: ResultRouteConfig,
        V: EventEmitter<O>,
    {
        let (tx, rx) = oneshot::channel();
        let sender = ResultSender::new(tx);

        if let Some(prev) = self.entries.last_mut() {
            prev.set_lifecycle(ScreenLifecycle::Paused);
        }

        let entity = cx.new(|cx| factory(&config, sender, cx));
        let subscription = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
            on_output(parent, child, event, cx);
        });

        self.entries.push(NavEntry::new(config, entity, Some(subscription)));
        cx.notify();
        rx
    }

    /// Pops the top screen from the stack if there is more than 1 screen.
    ///
    /// - Popped screen transitions to `ScreenLifecycle::Destroyed` and is dropped.
    /// - New top screen transitions back to `ScreenLifecycle::Resumed`.
    /// - Calls `cx.notify()`.
    ///
    /// Returns `true` if a screen was popped, `false` if at the root.
    pub fn pop<P: 'static>(&mut self, cx: &mut Context<P>) -> bool {
        if self.entries.len() > 1 {
            if let Some(mut popped) = self.entries.pop() {
                popped.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            if let Some(top) = self.entries.last_mut() {
                top.set_lifecycle(ScreenLifecycle::Resumed);
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Pops the top screen and restores focus to the previous screen.
    pub fn pop_and_focus<P: 'static>(&mut self, window: &mut Window, cx: &mut Context<P>) -> bool
    where
        V: NavFocusable,
    {
        if self.entries.len() > 1 {
            if let Some(mut popped) = self.entries.pop() {
                popped.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            if let Some(top) = self.entries.last_mut() {
                top.set_lifecycle(ScreenLifecycle::Resumed);
                if let Some(saved) = top.take_saved_focus() {
                    window.focus(&saved, cx);
                } else if let Some(handle) = top.entity.read(cx).focus_handle(cx) {
                    window.focus(&handle, cx);
                }
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Replaces the top entry with a new configuration.
    pub fn replace_top<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O>,
    {
        if let Some(mut old) = self.entries.pop() {
            old.set_lifecycle(ScreenLifecycle::Destroyed);
        }
        self.push(config, cx, factory, on_output);
    }

    /// Clears all screens from the stack.
    pub fn clear<P: 'static>(&mut self, cx: &mut Context<P>) {
        if !self.entries.is_empty() {
            for mut entry in self.entries.drain(..) {
                entry.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            cx.notify();
        }
    }

    /// Pops the top screen, allowing the stack to become empty if only 1 screen remains.
    pub fn pop_or_clear<P: 'static>(&mut self, cx: &mut Context<P>) -> bool {
        if let Some(mut popped) = self.entries.pop() {
            popped.set_lifecycle(ScreenLifecycle::Destroyed);
            if let Some(top) = self.entries.last_mut() {
                top.set_lifecycle(ScreenLifecycle::Resumed);
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Clears the entire stack and pushes the given configuration as the new root.
    pub fn clear_and_push<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O>,
    {
        for mut entry in self.entries.drain(..) {
            entry.set_lifecycle(ScreenLifecycle::Destroyed);
        }
        self.push(config, cx, factory, on_output);
    }

    /// Pops all screens down to the root screen.
    pub fn pop_to_root<P: 'static>(&mut self, cx: &mut Context<P>) -> bool {
        if self.entries.len() > 1 {
            let mut removed = self.entries.split_off(1);
            for mut entry in removed.drain(..) {
                entry.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            if let Some(root) = self.entries.first_mut() {
                root.set_lifecycle(ScreenLifecycle::Resumed);
            }
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Declarative reconciliation of the navigation stack.
    ///
    /// Accepts a target slice `&[C]`:
    /// - Reuses existing live `Entity<V>` for matching configs.
    /// - Creates only new entries for configs not previously present in the stack.
    /// - Marks removed entries as `Destroyed` and cancels subscriptions via RAII.
    /// - Updates stack order to match `target`.
    pub fn sync_stack<P: 'static, O: 'static>(
        &mut self,
        target: &[C],
        cx: &mut Context<P>,
        mut factory: impl FnMut(&C, &mut Context<V>) -> V,
        on_output: impl Fn(&mut P, Entity<V>, &O, &mut Context<P>) + 'static + Clone,
    ) where
        V: EventEmitter<O>,
    {
        if target.is_empty() {
            self.clear(cx);
            return;
        }

        let mut old_entries = std::mem::take(&mut self.entries);
        let mut new_entries = Vec::with_capacity(target.len());

        for (i, config) in target.iter().enumerate() {
            let is_top = i == target.len() - 1;
            let target_lifecycle = if is_top {
                ScreenLifecycle::Resumed
            } else {
                ScreenLifecycle::Paused
            };

            if let Some(pos) = old_entries.iter().position(|e| &e.config == config) {
                let mut entry = old_entries.swap_remove(pos);
                entry.set_lifecycle(target_lifecycle);
                new_entries.push(entry);
            } else {
                let entity = cx.new(|cx| factory(config, cx));
                let cb = on_output.clone();
                let sub = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
                    cb(parent, child, event, cx);
                });
                let mut entry = NavEntry::new(config.clone(), entity, Some(sub));
                entry.set_lifecycle(target_lifecycle);
                new_entries.push(entry);
            }
        }

        // Any remaining old entries were evicted
        for mut old in old_entries {
            old.set_lifecycle(ScreenLifecycle::Destroyed);
        }

        self.entries = new_entries;
        cx.notify();
    }

    /// Requests focus on the currently active screen.
    pub fn focus_active(&self, window: &mut Window, cx: &mut App) -> bool
    where
        V: NavFocusable,
    {
        if let Some(entry) = self.entries.last()
            && let Some(handle) = entry.entity.read(cx).focus_handle(cx)
        {
            window.focus(&handle, cx);
            return true;
        }
        false
    }

    /// Returns `true` if there is a screen to pop back to.
    pub fn can_go_back(&self) -> bool {
        self.entries.len() > 1
    }

    /// Returns the configuration of the top screen.
    pub fn current_config(&self) -> Option<&C> {
        self.entries.last().map(|e| &e.config)
    }

    /// Returns the live entity of the top screen.
    pub fn active_entity(&self) -> Option<&Entity<V>> {
        self.entries.last().map(|e| &e.entity)
    }

    /// Returns the total number of screens in the stack.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns a list of all route configurations currently in the stack.
    pub fn stack_configs(&self) -> Vec<C> {
        self.entries.iter().map(|e| e.config.clone()).collect()
    }

    /// Renders the currently active (top) screen of the stack.
    pub fn render_active(&self) -> AnyElement {
        if let Some(entry) = self.entries.last() {
            entry.entity.clone().into_any_element()
        } else {
            div().into_any_element()
        }
    }
}

impl<C: RouteConfig, V: Render + 'static> NavigationNode for StackNavigator<C, V> {
    fn node_id(&self) -> NavigationNodeId {
        self.node_id
    }

    fn can_pop(&self) -> bool {
        self.entries.len() > 1
    }

    fn handle_pop(&mut self, _window: &mut Window, _cx: &mut App) -> NavigationFlow {
        if self.entries.len() > 1 {
            if let Some(mut popped) = self.entries.pop() {
                popped.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            if let Some(top) = self.entries.last_mut() {
                top.set_lifecycle(ScreenLifecycle::Resumed);
            }
            NavigationFlow::Handled
        } else {
            NavigationFlow::BubbleUp
        }
    }
}
