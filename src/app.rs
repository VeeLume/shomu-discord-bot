use anyhow::{Context as AnyhowContext, Result};
use poise::Framework;
use serenity::all::{ClientBuilder, GatewayIntents, GuildId};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::commands::{lookup_ac, settings, stats, userinfo};
use crate::events::event_handler;
use crate::state::AppState;

pub async fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let token = std::env::var("DISCORD_TOKEN").context("Set DISCORD_TOKEN in env")?;
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://bot.db".into());

    let token_tail = token
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    info!("Starting bot with DB: {db_url}");
    info!("Discord token: ...{token_tail} (len={})", token.len());

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
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                // Register commands in a specific guild for faster iteration during development
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

    // --- Serenity client ---
    // Build the client and attach the framework
    let mut client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .context("Building serenity client failed")?;

    // This should BLOCK until the shard runner ends (or we get a fatal error).
    // If it returns, we log and return an Err so Docker sees a non-zero exit.
    info!("Connecting to Discord gateway…");
    if let Err(e) = client.start().await {
        // Network/auth/config error -> fail non-zero
        return Err(anyhow::anyhow!("Discord client error: {e:#}"));
    }

    // If we got here, the shard runner ended gracefully.
    // Treat this as an error so the container doesn't loop silently.
    Err(anyhow::anyhow!(
        "Discord client stopped unexpectedly without error"
    ))
}
