use dioxus::prelude::*;
// use crate::pages::wpt_fyi::WptFyiPage;
use crate::pages::home::HomePage;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Router::<Route> {}
    }
}

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Page)]
    #[route("/")]
    HomePage {},
    // #[route("/wpt-fyi")]
    // WptFyiPage {},
}

#[component]
fn Page() -> Element {
    rsx! {
        NavBar {}
        Outlet::<Route> {}
    }
}

#[component]
fn NavBar() -> Element {
    rsx! {
        div {
            id: "navbar",
            Link {
                to: Route::HomePage {},
                "Home"
            }
            // Link {
            //     to: Route::WptFyiPage {},
            //     "WPT.FYI"
            // }
        }
    }
}