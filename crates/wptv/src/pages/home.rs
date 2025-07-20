use dioxus::prelude::*;

/// Home page
#[component]
pub fn HomePage() -> Element {
    rsx! {
        div {
            "hello world"
        }
    }
}