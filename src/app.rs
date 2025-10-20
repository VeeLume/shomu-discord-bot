use anyhow::{Context as AnyhowContext, Result};
use poise::{Framework};
use serenity::all::{GatewayIntents, GuildId, ClientBuilder};
use tracing_subscriber::EnvFilter;

use crate::commands::{settings, userinfo, stats, lookup_ac};
use crate::events::event_handler;
use crate::state::AppState;


pub async fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let token = std::env::var("DISCORD_TOKEN")
        .context("Set DISCORD_TOKEN in env")?;
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://bot.db".into());

    let intents = GatewayIntents::GUILD_MEMBERS;

    let framework = Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                userinfo::userinfo(),
                settings::settings(),
                lookup_ac::userinfo_lookup(),
                stats::stats_rejoiners(),
                stats::stats_exits(),
                stats::stats_current(),
                stats::stats_member_balance(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                // NOTE: if you want global commands, change this.
                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    GuildId::new(1429268494687408232),
                )
                .await?;
                AppState::new(&db_url).await
            })
        })
        .build();

    let client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
    Ok(())
}
