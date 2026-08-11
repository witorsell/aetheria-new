mod api;
mod app;
mod components;
mod pages;
mod render;

use app::App;

fn main() {
    leptos::mount::mount_to_body(App);
}
