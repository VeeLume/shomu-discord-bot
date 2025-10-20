use anyhow::Result;
use poise::serenity_prelude as serenity;

use crate::flows::settings_panel::SettingsPanel;
use crate::flows::{self};
use crate::repos::GuildSettingsRepo;
use crate::state::Ctx;

#[derive(poise::ChoiceParameter, Copy, Clone, Debug)]
pub enum LogKind {
    #[name = "join"]
    Join,
    #[name = "leave"]
    Leave,
    #[name = "mod"]
    Mod,
}

#[poise::command(slash_command, guild_only, required_permissions = "MANAGE_GUILD")]
pub async fn setlog(
    ctx: Ctx<'_>,
    #[description = "Which log to set"] kind: LogKind,
    #[description = "Channel to post logs to"] channel: serenity::Channel,
) -> Result<()> {
    let ch_id = channel.id();
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => {
            ctx.say("This command can only be used in a guild.").await?;
            return Ok(());
        }
    };

    let repo = GuildSettingsRepo::new(&ctx.data().db);

    match kind {
        LogKind::Join => repo.upsert(&guild_id, Some(ch_id), None, None).await?,
        LogKind::Leave => repo.upsert(&guild_id, None, Some(ch_id), None).await?,
        LogKind::Mod => repo.upsert(&guild_id, None, None, Some(ch_id)).await?,
    }

    ctx.say(format!(
        "Set **{kind:?}** log channel to <#{}>.",
        ch_id.get()
    ))
    .await?;
    Ok(())
}

#[poise::command(slash_command, guild_only, required_permissions = "MANAGE_GUILD")]
pub async fn settings(ctx: Ctx<'_>) -> anyhow::Result<()> {
    let gid = ctx.guild_id().unwrap();
    let author = ctx.author().id;
    let state = ctx.data().clone();
    let current = crate::repos::GuildSettingsRepo::new(&state.db).get(&gid).await?;

    let panel = crate::flows::settings_panel::SettingsPanel::new(state, gid, author, current);

    // Attached + ephemeral; 3 minute timeout
    crate::flows::run(
        ctx.serenity_context(),
        crate::flows::Surface::AttachedEphemeral,
        panel,
        Some(ctx),
        180,
        false,
    ).await
}
