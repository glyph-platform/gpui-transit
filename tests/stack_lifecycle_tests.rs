use gpui::prelude::*;
use gpui::{Context, EventEmitter, IntoElement, Render, Window, div};
use gpui_transit::StackNavigator;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum TestRoute {
    Home,
    Detail(u32),
}

struct TestScreen {
    name: String,
}

impl Render for TestScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.name.clone())
    }
}

#[derive(Clone, Debug)]
struct TestOutput;

impl EventEmitter<TestOutput> for TestScreen {}

struct RootHost;

impl Render for RootHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn test_stack_basic_logic(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let parent = cx.new(|_| RootHost);
        parent.update(cx, |_host, cx| {
            let mut stack: StackNavigator<TestRoute, TestScreen> = StackNavigator::new_empty();

            assert!(stack.is_empty());
            assert_eq!(stack.len(), 0);
            assert!(!stack.can_go_back());

            let pushed = stack.push(
                TestRoute::Home,
                cx,
                |_route, _cx| TestScreen { name: "Home".into() },
                |_parent, _child, _event: &TestOutput, _cx| {},
            );
            assert!(pushed);
            assert_eq!(stack.len(), 1);
            assert_eq!(stack.current_config(), Some(&TestRoute::Home));

            // Duplicate rejected
            let dup = stack.push(
                TestRoute::Home,
                cx,
                |_route, _cx| TestScreen { name: "Home".into() },
                |_parent, _child, _event: &TestOutput, _cx| {},
            );
            assert!(!dup);
            assert_eq!(stack.len(), 1);

            // Push detail
            let pushed_detail = stack.push(
                TestRoute::Detail(42),
                cx,
                |_route, _cx| TestScreen { name: "Detail 42".into() },
                |_parent, _child, _event: &TestOutput, _cx| {},
            );
            assert!(pushed_detail);
            assert_eq!(stack.len(), 2);
            assert!(stack.can_go_back());
            assert_eq!(stack.current_config(), Some(&TestRoute::Detail(42)));

            // Pop
            let popped = stack.pop(cx);
            assert!(popped);
            assert_eq!(stack.len(), 1);
            assert_eq!(stack.current_config(), Some(&TestRoute::Home));
            assert!(!stack.can_go_back());
        });
    });
}
