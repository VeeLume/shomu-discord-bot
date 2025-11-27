use anyhow::{Context as AnyhowContext, Result};
use poise::Framework;
use serenity::all::{CacheHttp, ClientBuilder, GatewayIntents, GuildId};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::commands::{member, settings, stats, userinfo};
use crate::events::event_handler;
use crate::state::AppState;

pub async fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let token = std::env::var("DISCORD_TOKEN").context("Set DISCORD_TOKEN in env")?;
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://bot.db".into());
    let test_guild = std::env::var("TEST_GUILD_ID").ok();

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

    let intents = GatewayIntents::GUILD_MEMBERS | GatewayIntents::non_privileged();

    let framework = Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                userinfo::userinfo(),
                settings::settings(),
                member::member(),
                stats::stats(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                // only register commands if no test guild is set

                match poise::builtins::register_globally(ctx, &framework.options().commands).await {
                    Ok(_) => info!("Registered application commands globally"),
                    Err(e) => {
                        eprintln!("Failed to register application commands globally: {e:#}")
                    }
                }

                // Register commands in a specific guild for faster iteration during development
                if let Some(gid) = test_guild.as_ref() {
                    let gid = gid
                        .parse::<u64>()
                        .context("TEST_GUILD_ID must be a valid u64")?;
                    match poise::builtins::register_in_guild(
                        ctx,
                        &framework.options().commands,
                        GuildId::new(gid),
                    )
                    .await
                    {
                        Ok(_) => info!("Registered application commands in test guild {gid}"),
                        Err(e) => {
                            eprintln!(
                                "Failed to register application commands in test guild {gid}: {e:#}"
                            )
                        }
                    }
                }

                match ctx.http().get_global_commands().await {
                    Ok(cmds) => {
                        info!("Currently registered global commands:");
                        for cmd in cmds {
                            info!(" - {} (ID {})", cmd.name, cmd.id);
                        }
                    }
                    Err(e) => eprintln!("Failed to fetch global commands: {e:#}"),
                }

                AppState::new(&db_url).await
            })
        })
        .build();

    let mut client = ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .context("Building serenity client failed")?;

    info!("Connecting to Discord gateway…");
    if let Err(e) = client.start().await {
        // Network/auth/config error -> fail non-zero
        return Err(anyhow::anyhow!("Discord client error: {e:#}"));
    }

    info!("Discord client disconnected gracefully.");
    Ok(())
}
