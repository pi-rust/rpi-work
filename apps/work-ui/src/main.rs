//! Application entry point. Runtime coordination lives in app; screens in features.
mod app;
mod features;
mod shared;

fn main() {
    app::launch();
}
