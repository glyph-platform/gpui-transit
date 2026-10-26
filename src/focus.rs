use gpui::{App, FocusHandle, Window};

/// Trait for components that can participate in automatic navigation focus management.
pub trait NavFocusable {
    /// Returns the primary `FocusHandle` for this component.
    fn focus_handle(&self, _cx: &App) -> Option<FocusHandle> {
        None
    }
}

/// A dedicated focus history stack ensuring focus is never lost across push, pop,
/// and modal presentation operations.
#[derive(Debug, Default)]
pub struct FocusStack {
    stack: Vec<FocusHandle>,
}

impl FocusStack {
    /// Creates a new empty focus stack.
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Pushes the currently focused element onto the stack if available.
    pub fn push_current(&mut self, window: &Window, cx: &App) {
        if let Some(handle) = window.focused(cx) {
            self.stack.push(handle);
        }
    }

    /// Pushes a specific focus handle onto the stack.
    pub fn push(&mut self, handle: FocusHandle) {
        self.stack.push(handle);
    }

    /// Pops the topmost saved focus handle and restores focus to it.
    ///
    /// Returns `true` if a handle was restored, `false` if the stack was empty.
    pub fn pop_and_restore(&mut self, window: &mut Window, cx: &mut App) -> bool {
        if let Some(handle) = self.stack.pop() {
            window.focus(&handle, cx);
            true
        } else {
            false
        }
    }

    /// Clears the focus stack.
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Returns `true` if the focus stack is empty.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Returns the depth of the saved focus stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

/// Helper extension trait to focus focusable entities or values.
pub trait WindowFocusExt {
    /// Requests focus on the given focusable target.
    fn focus_target<F: NavFocusable>(&mut self, target: &F, cx: &mut App) -> bool;

    /// Saves the currently focused element into the given focus stack, then focuses the target.
    fn focus_and_save<F: NavFocusable>(
        &mut self,
        target: &F,
        stack: &mut FocusStack,
        cx: &mut App,
    ) -> bool;
}

impl WindowFocusExt for Window {
    fn focus_target<F: NavFocusable>(&mut self, target: &F, cx: &mut App) -> bool {
        if let Some(handle) = target.focus_handle(cx) {
            self.focus(&handle, cx);
            true
        } else {
            false
        }
    }

    fn focus_and_save<F: NavFocusable>(
        &mut self,
        target: &F,
        stack: &mut FocusStack,
        cx: &mut App,
    ) -> bool {
        stack.push_current(self, cx);
        self.focus_target(target, cx)
    }
}
