use gpui::prelude::*;
use gpui::{Context, EventEmitter, IntoElement, Render, Window, div};
use gpui_transit::SlotNavigator;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct ChatRoute(String);

struct ChatScreen {
    id: String,
}

impl Render for ChatScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.id.clone())
    }
}

#[derive(Clone, Debug)]
struct ChatOutput;

impl EventEmitter<ChatOutput> for ChatScreen {}

struct RootHost;

impl Render for RootHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn test_slot_navigator_lru_caching_and_lifecycle(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let parent = cx.new(|_| RootHost);
        parent.update(cx, |_host, cx| {
            // LRU capacity = 2 cached screens
            let mut slot: SlotNavigator<ChatRoute, ChatScreen> = SlotNavigator::new(2);

            assert_eq!(slot.capacity(), 2);
            assert_eq!(slot.cache_len(), 0);
            assert!(slot.active_config().is_none());

            // 1. Activate Chat "A"
            slot.activate(
                ChatRoute("A".into()),
                cx,
                |r, _cx| ChatScreen { id: r.0.clone() },
                |_p, _c, _e: &ChatOutput, _cx| {},
            );
            assert_eq!(slot.active_config(), Some(&ChatRoute("A".into())));
            assert_eq!(slot.cache_len(), 0);
            assert!(slot.contains(&ChatRoute("A".into())));

            // 2. Activate Chat "B" (moves "A" into LRU cache)
            slot.activate(
                ChatRoute("B".into()),
                cx,
                |r, _cx| ChatScreen { id: r.0.clone() },
                |_p, _c, _e: &ChatOutput, _cx| {},
            );
            assert_eq!(slot.active_config(), Some(&ChatRoute("B".into())));
            assert_eq!(slot.cache_len(), 1);
            assert!(slot.contains(&ChatRoute("A".into())));
            assert!(slot.contains(&ChatRoute("B".into())));

            // 3. Activate Chat "C" (moves "B" into LRU cache. Cache now has A and B)
            slot.activate(
                ChatRoute("C".into()),
                cx,
                |r, _cx| ChatScreen { id: r.0.clone() },
                |_p, _c, _e: &ChatOutput, _cx| {},
            );
            assert_eq!(slot.active_config(), Some(&ChatRoute("C".into())));
            assert_eq!(slot.cache_len(), 2);

            // 4. Activate Chat "D" (capacity 2 exceeded -> evicts least recently used "A")
            slot.activate(
                ChatRoute("D".into()),
                cx,
                |r, _cx| ChatScreen { id: r.0.clone() },
                |_p, _c, _e: &ChatOutput, _cx| {},
            );
            assert_eq!(slot.active_config(), Some(&ChatRoute("D".into())));
            assert_eq!(slot.cache_len(), 2);
            // "A" should be evicted and no longer contained
            assert!(!slot.contains(&ChatRoute("A".into())));
            assert!(slot.contains(&ChatRoute("B".into())));
            assert!(slot.contains(&ChatRoute("C".into())));
            assert!(slot.contains(&ChatRoute("D".into())));

            // 5. Promote "B" from cache to active
            slot.activate(
                ChatRoute("B".into()),
                cx,
                |r, _cx| ChatScreen { id: r.0.clone() },
                |_p, _c, _e: &ChatOutput, _cx| {},
            );
            assert_eq!(slot.active_config(), Some(&ChatRoute("B".into())));
            assert_eq!(slot.cache_len(), 2); // now holds D and C

            // 6. Clear active slot
            slot.clear(cx);
            assert!(slot.active_config().is_none());
            assert_eq!(slot.cache_len(), 2);

            // 7. Clear all
            slot.clear_all(cx);
            assert!(slot.active_config().is_none());
            assert_eq!(slot.cache_len(), 0);
        });
    });
}
