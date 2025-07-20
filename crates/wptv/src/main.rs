// pub mod wpt_fyi_api;
pub mod pages;
pub mod components;

fn main() {
    dioxus::launch(components::app::App);
}
