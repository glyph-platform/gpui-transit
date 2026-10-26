use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, IntoElement,
    ParentElement, Pixels, Render, Styled, Window, div, rgba,
};

use crate::focus::{FocusStack, NavFocusable};
use crate::lifecycle::ScreenLifecycle;
use crate::route::{NavEntry, RouteConfig};
use crate::tree::{NavigationFlow, NavigationNode, NavigationNodeId};

/// Visual presentation style for an overlay layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayKind {
    /// Centered modal dialog with a dimmed background backdrop.
    #[default]
    ModalDialog,

    /// Side drawer panel sliding from the right edge.
    SideSheetRight,

    /// Side drawer panel sliding from the left edge.
    SideSheetLeft,

    /// Popover card positioned relative to an anchor point or button.
    Popover { x: Pixels, y: Pixels },

    /// Floating toast banner at the top-right corner.
    Toast,
}

/// Advanced modal overlay navigator for dialogs, side-sheets, popovers, and toasts (`OverlayNavigator`).
///
/// Features:
/// - Supports stacked modal dialogs, drawers, and popovers.
/// - **Deep Focus Stack**: Uses `FocusStack` to retain every intermediate focus handle so focus
///   is never lost even with nested confirmations (e.g. Modal 1 -> Confirm Dialog 2).
/// - **Backdrop Handling**: Outside-click dismissal with propagation guards.
/// - **Navigation Tree Integration**: Implements `NavigationNode` so pressing `Escape` or Back
///   dismisses the topmost overlay before any background stack is popped.
pub struct OverlayNavigator<C: RouteConfig, V: Render + 'static> {
    entries: Vec<NavEntry<C, V>>,
    overlay_kinds: Vec<OverlayKind>,
    focus_stack: FocusStack,
    node_id: NavigationNodeId,
}

impl<C: RouteConfig, V: Render + 'static> Default for OverlayNavigator<C, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: RouteConfig, V: Render + 'static> OverlayNavigator<C, V> {
    /// Creates an empty overlay navigator.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            overlay_kinds: Vec::new(),
            focus_stack: FocusStack::new(),
            node_id: NavigationNodeId::next(),
        }
    }

    /// Presents a new modal dialog on top of the overlay stack.
    pub fn present<P: 'static, O: 'static>(
        &mut self,
        config: C,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O>,
    {
        self.present_kind(config, OverlayKind::ModalDialog, cx, factory, on_output);
    }

    /// Presents an overlay with a specific visual presentation style (Modal, Drawer, Popover, Toast).
    pub fn present_kind<P: 'static, O: 'static>(
        &mut self,
        config: C,
        kind: OverlayKind,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O>,
    {
        let entity = cx.new(|cx| factory(&config, cx));
        let subscription = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
            on_output(parent, child, event, cx);
        });

        self.entries.push(NavEntry::new(config, entity, Some(subscription)));
        self.overlay_kinds.push(kind);
        cx.notify();
    }

    /// Presents a modal dialog and transfers keyboard focus to it, recording current focus.
    pub fn present_and_focus<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O> + NavFocusable,
    {
        self.focus_stack.push_current(window, cx);
        self.present(config, cx, factory, on_output);

        if let Some(top) = self.entries.last()
            && let Some(handle) = top.entity.read(cx).focus_handle(cx)
        {
            window.focus(&handle, cx);
        }
    }

    /// Presents a modal dialog with `Window` access.
    pub fn present_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        mut on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O>,
    {
        let entity = cx.new(|cx| factory(&config, window, cx));
        let subscription = cx.subscribe(&entity, move |parent, child, event: &O, cx| {
            on_output(parent, child, event, cx);
        });

        self.entries.push(NavEntry::new(config, entity, Some(subscription)));
        self.overlay_kinds.push(OverlayKind::ModalDialog);
        cx.notify();
    }

    /// Presents a modal dialog with `Window` access and transfers keyboard focus.
    pub fn present_and_focus_in<P: 'static, O: 'static>(
        &mut self,
        config: C,
        window: &mut Window,
        cx: &mut Context<P>,
        factory: impl FnOnce(&C, &mut Window, &mut Context<V>) -> V,
        on_output: impl FnMut(&mut P, Entity<V>, &O, &mut Context<P>) + 'static,
    ) where
        V: EventEmitter<O> + NavFocusable,
    {
        self.focus_stack.push_current(window, cx);
        self.present_in(config, window, cx, factory, on_output);

        if let Some(top) = self.entries.last()
            && let Some(handle) = top.entity.read(cx).focus_handle(cx)
        {
            window.focus(&handle, cx);
        }
    }

    /// Dismisses the top-most modal overlay.
    pub fn dismiss_top<P: 'static>(&mut self, cx: &mut Context<P>) -> bool {
        if let Some(mut popped) = self.entries.pop() {
            self.overlay_kinds.pop();
            popped.set_lifecycle(ScreenLifecycle::Destroyed);
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Dismisses the top modal and restores focus using the focus history stack.
    pub fn dismiss_top_and_restore_focus<P: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<P>,
    ) -> bool {
        let dismissed = self.dismiss_top(cx);
        if dismissed {
            self.focus_stack.pop_and_restore(window, cx);
        }
        dismissed
    }

    /// Dismisses all overlays matching the specified configuration.
    pub fn dismiss_by_config<P: 'static>(&mut self, config: &C, cx: &mut Context<P>) -> bool {
        let mut idx = 0;
        let mut removed = false;
        while idx < self.entries.len() {
            if &self.entries[idx].config == config {
                let mut entry = self.entries.remove(idx);
                self.overlay_kinds.remove(idx);
                entry.set_lifecycle(ScreenLifecycle::Destroyed);
                removed = true;
            } else {
                idx += 1;
            }
        }
        if removed {
            cx.notify();
        }
        removed
    }

    /// Dismisses all open overlays.
    pub fn dismiss_all<P: 'static>(&mut self, cx: &mut Context<P>) {
        if !self.entries.is_empty() {
            for mut entry in self.entries.drain(..) {
                entry.set_lifecycle(ScreenLifecycle::Destroyed);
            }
            self.overlay_kinds.clear();
            self.focus_stack.clear();
            cx.notify();
        }
    }

    /// Returns `true` if at least one overlay is presented.
    pub fn is_presented(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Returns `true` if no overlay is currently presented.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the configuration of the top modal.
    pub fn top_config(&self) -> Option<&C> {
        self.entries.last().map(|e| &e.config)
    }

    /// Returns the entity of the top modal.
    pub fn top_entity(&self) -> Option<&Entity<V>> {
        self.entries.last().map(|e| &e.entity)
    }

    /// Total number of stacked overlays.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Renders the modal overlay layer:
    /// - Dimmed semi-transparent backdrop.
    /// - Modal card container positioned according to its `OverlayKind`.
    /// - Stops click-propagation inside the card to prevent unintended backdrop dismissal.
    pub fn render_overlay(
        &self,
        on_dismiss_backdrop: Option<impl Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    ) -> Option<AnyElement> {
        let top_entry = self.entries.last()?;
        let top_kind = self.overlay_kinds.last().copied().unwrap_or_default();
        let child_element = top_entry.entity.clone();

        let mut backdrop = div()
            .id("modal-backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000080));

        if let Some(on_dismiss) = on_dismiss_backdrop {
            backdrop = backdrop.on_click(on_dismiss);
        }

        let modal_card = div()
            .id("modal-card-content")
            .on_click(|_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(child_element);

        let container = match top_kind {
            OverlayKind::ModalDialog => backdrop
                .flex()
                .items_center()
                .justify_center()
                .child(modal_card),
            OverlayKind::SideSheetRight => backdrop
                .flex()
                .justify_end()
                .child(modal_card.h_full()),
            OverlayKind::SideSheetLeft => backdrop
                .flex()
                .justify_start()
                .child(modal_card.h_full()),
            OverlayKind::Popover { x, y } => backdrop
                .child(modal_card.absolute().left(x).top(y)),
            OverlayKind::Toast => backdrop
                .flex()
                .justify_end()
                .items_start()
                .p_4()
                .child(modal_card),
        };

        Some(container.into_any_element())
    }
}

impl<C: RouteConfig, V: Render + 'static> NavigationNode for OverlayNavigator<C, V> {
    fn node_id(&self) -> NavigationNodeId {
        self.node_id
    }

    fn can_pop(&self) -> bool {
        !self.entries.is_empty()
    }

    fn handle_pop(&mut self, window: &mut Window, cx: &mut App) -> NavigationFlow {
        if let Some(mut popped) = self.entries.pop() {
            self.overlay_kinds.pop();
            popped.set_lifecycle(ScreenLifecycle::Destroyed);
            self.focus_stack.pop_and_restore(window, cx);
            NavigationFlow::Handled
        } else {
            NavigationFlow::BubbleUp
        }
    }
}
