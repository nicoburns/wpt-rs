// pub mod wpt_fyi_api;
pub mod components;
pub mod pages;

fn main() {
    dioxus::launch(components::app::App);
}
