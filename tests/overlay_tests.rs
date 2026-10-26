use gpui::prelude::*;
use gpui::{Context, EventEmitter, IntoElement, Render, Window, div};
use gpui_transit::{OverlayKind, OverlayNavigator};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum ModalRoute {
    Settings,
    ConfirmDelete,
    ToastNotice,
}

struct ModalScreen {
    title: String,
}

impl Render for ModalScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.title.clone())
    }
}

#[derive(Clone, Debug)]
struct ModalOutput;

impl EventEmitter<ModalOutput> for ModalScreen {}

struct RootHost;

impl Render for RootHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn test_overlay_navigator_presentation_and_kinds(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let parent = cx.new(|_| RootHost);
        parent.update(cx, |_host, cx| {
            let mut overlay: OverlayNavigator<ModalRoute, ModalScreen> = OverlayNavigator::new();

            assert!(!overlay.is_presented());
            assert_eq!(overlay.len(), 0);

            // 1. Present Settings Modal
            overlay.present_kind(
                ModalRoute::Settings,
                OverlayKind::ModalDialog,
                cx,
                |_r, _cx| ModalScreen { title: "Settings".into() },
                |_p, _c, _e: &ModalOutput, _cx| {},
            );

            assert!(overlay.is_presented());
            assert_eq!(overlay.len(), 1);
            assert_eq!(overlay.top_config(), Some(&ModalRoute::Settings));

            // 2. Present Confirmation Dialog on top
            overlay.present_kind(
                ModalRoute::ConfirmDelete,
                OverlayKind::ModalDialog,
                cx,
                |_r, _cx| ModalScreen { title: "Confirm".into() },
                |_p, _c, _e: &ModalOutput, _cx| {},
            );

            assert_eq!(overlay.len(), 2);
            assert_eq!(overlay.top_config(), Some(&ModalRoute::ConfirmDelete));

            // 3. Dismiss top (ConfirmDelete)
            let dismissed = overlay.dismiss_top(cx);
            assert!(dismissed);
            assert_eq!(overlay.len(), 1);
            assert_eq!(overlay.top_config(), Some(&ModalRoute::Settings));

            // 4. Present Toast
            overlay.present_kind(
                ModalRoute::ToastNotice,
                OverlayKind::Toast,
                cx,
                |_r, _cx| ModalScreen { title: "Notice".into() },
                |_p, _c, _e: &ModalOutput, _cx| {},
            );
            assert_eq!(overlay.len(), 2);

            // 5. Dismiss by config
            let dismissed_settings = overlay.dismiss_by_config(&ModalRoute::Settings, cx);
            assert!(dismissed_settings);
            assert_eq!(overlay.len(), 1);
            assert_eq!(overlay.top_config(), Some(&ModalRoute::ToastNotice));

            // 6. Dismiss all
            overlay.dismiss_all(cx);
            assert!(!overlay.is_presented());
            assert_eq!(overlay.len(), 0);
        });
    });
}
