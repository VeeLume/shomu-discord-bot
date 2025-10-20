use std::sync::Arc;

use anyhow::Result;
use poise::FrameworkContext;
use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, CreateEmbed, CreateMessage, GuildId, Timestamp, User, UserId};
use serenity::prelude::Context;

use crate::repos::{GuildSettingsRepo, MembershipsRepo};
use crate::state::AppState;

pub async fn event_handler(
    ctx: &Context,
    event: &serenity::FullEvent,
    _framework: FrameworkContext<'_, Arc<AppState>, anyhow::Error>,
    state: &Arc<AppState>,
) -> Result<()> {
    use serenity::FullEvent::*;
    match event {
        Ready { data_about_bot, .. } => handle_ready(ctx, state, data_about_bot).await?,
        GuildMemberAddition { new_member } => on_join(ctx, state, new_member).await?,
        GuildMemberRemoval { guild_id, user, .. } => on_leave(ctx, state, guild_id, user).await?,
        GuildBanAddition {
            guild_id,
            banned_user,
        } => on_guild_ban_add(state, *guild_id, banned_user).await?,
        _ => {}
    }
    Ok(())
}

async fn post_embed(
    http: &serenity::http::Http,
    channel: Option<ChannelId>,
    title: &str,
    f: impl FnOnce(CreateEmbed) -> CreateEmbed,
) {
    if let Some(ch) = channel {
        let _ = ch
            .send_message(
                http,
                CreateMessage::new().embed(f(CreateEmbed::new().title(title))),
            )
            .await;
    }
}

pub async fn handle_ready(
    _ctx: &Context,
    state: &Arc<AppState>,
    ready: &serenity::Ready,
) -> Result<()> {
    tracing::info!("Connected as {}", ready.user.name);

    let mrepo = MembershipsRepo::new(&state.db);
    for guild in &ready.guilds {
        tracing::info!("Connected to guild: {}", guild.id);
        mrepo.rebuild_usernames_fts_for_guild(guild.id).await.map_err(
            |e| tracing::warn!("Failed to rebuild usernames FTS for guild {}: {}", guild.id, e)
        ).ok();
    }

    // Light maintenance loop for recent_bans
    let state_clone = state.clone();
    tokio::spawn(async move {
        let every_min = std::time::Duration::from_secs(60);
        loop {
            state_clone.prune_recent_bans(60);
            tokio::time::sleep(every_min).await;
        }
    });

    Ok(())
}

/// Join: just persist basic info; no invites needed.
pub async fn on_join(
    ctx: &Context,
    state: &AppState,
    member: &serenity::all::Member,
) -> Result<()> {
    let guild_id = member.guild_id;
    let user_id = member.user.id;

    let mrepo = MembershipsRepo::new(&state.db);
    mrepo.record_join(guild_id, member).await?;
    mrepo.upsert_usernames_fts_row(guild_id, &user_id.to_string()).await?;

    let grepo = GuildSettingsRepo::new(&state.db);
    let settings = grepo.get(&guild_id).await?;

    post_embed(&ctx.http, settings.join_log, "Member joined", |e| {
        e.description(format!("<@{}> joined.", user_id.get()))
            .timestamp(Timestamp::now())
    })
    .await;

    Ok(())
}

/// Leave: mark as banned if a recent `GuildBanAdd` was seen; else left.
pub async fn on_leave(
    ctx: &Context,
    state: &AppState,
    guild_id: &GuildId,
    user: &User,
) -> Result<()> {
    let banned = state.was_recently_banned(*guild_id, user.id, 15);

    let mrepo = MembershipsRepo::new(&state.db);
    mrepo.record_leave(*guild_id, user.id, banned).await?;

    let grepo = GuildSettingsRepo::new(&state.db);
    let settings = grepo.get(guild_id).await?;
    let target = if banned {
        settings.mod_log.or(settings.leave_log)
    } else {
        settings.leave_log
    };

    post_embed(&ctx.http, target, "Member left", |e| {
        e.description(format!(
            "<@{}> {}.",
            user.id.get(),
            if banned { "was **banned**" } else { "left" }
        ))
        .timestamp(Timestamp::now())
    })
    .await;

    Ok(())
}

/// Record the ban so we can classify leaves without audit logs.
async fn on_guild_ban_add(state: &AppState, guild_id: GuildId, banned_user: &User) -> Result<()> {
    state.mark_recent_ban(guild_id, banned_user.id);

    // Optional: close open stint immediately as banned (best effort)
    let mrepo = MembershipsRepo::new(&state.db);
    let _ = mrepo.record_leave(guild_id, banned_user.id, true).await;
    Ok(())
}
