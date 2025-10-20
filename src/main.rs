mod app;
mod commands;
mod events;
mod invites;
mod state;
mod flows;
mod repos;
mod db;

#[tokio::main]
async fn main() {
    if let Err(e) = app::run().await {
        eprintln!("Fatal error: {e:#}");
        std::process::exit(1);
    }
}
