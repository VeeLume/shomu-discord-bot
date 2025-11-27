use anyhow::Result;
use poise::serenity_prelude as serenity;

use crate::commands::send_chunked_embeds;
use crate::repos::MembershipsRepo;
use crate::state::Ctx;

/// Autocomplete by nickname/account username; returns `AutocompleteChoice<label, value=user_id>`
pub async fn ac_member(ctx: Ctx<'_>, partial: &str) -> Vec<serenity::AutocompleteChoice> {
    let Some(gid) = ctx.guild_id() else {
        return Vec::new();
    };

    let repo = MembershipsRepo::new(&ctx.data().db);
    // Limit 25: Discord max visible suggestions
    let Ok(rows) = repo.search_user_summaries_prefix(gid, partial, 25).await else {
        return Vec::new();
    };

    rows.into_iter()
        .map(|r| {
            let label = match (r.server_username.as_deref(), r.account_username.as_deref()) {
                (Some(nick), Some(acc)) if !nick.is_empty() => format!("{nick} (aka {acc})"),
                (_, Some(acc)) => acc.to_string(),
                (Some(nick), None) => nick.to_string(),
                _ => format!("User {}", r.user_id),
            };
            // value = user_id (string). Keeps execution side simple/reliable even for ex-members.
            serenity::AutocompleteChoice::new(label, r.user_id)
        })
        .collect()
}

/// Parent command: `/member`
///
/// Right now it only exposes `/member history`, but you can add more later
/// (e.g. `/member search`, `/member summary`, etc.).
#[poise::command(
    slash_command,
    guild_only,
    ephemeral,
    subcommands("member_history"),
    rename = "member"
)]
pub async fn member(_: Ctx<'_>) -> Result<()> {
    Ok(())
}

/// Show the membership history for a user picked via autocomplete.
///
/// Usage: `/member history user:<type to search>`
/// (backed by FTS/LIKE search through `ac_member`).
#[poise::command(slash_command, guild_only, ephemeral, rename = "history")]
pub async fn member_history(
    ctx: Ctx<'_>,
    #[description = "Pick a user by name"]
    #[autocomplete = "ac_member"]
    user_id: String,
) -> Result<()> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("This command can only be used in a guild.").await?;
        return Ok(());
    };

    let repo = MembershipsRepo::new(&ctx.data().db);

    let uid = match user_id.parse::<u64>() {
        Ok(raw) => serenity::all::UserId::new(raw),
        Err(_) => {
            ctx.say("Couldn't parse that user id. Please pick from the autocomplete list.")
                .await?;
            return Ok(());
        }
    };

    let rows = repo.history_for_user(guild_id, uid).await?;

    // Helper for timestamps
    let ts = |rfc2822: &str| -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(rfc2822) {
            format!("<t:{}:f>", dt.timestamp())
        } else {
            rfc2822.to_string()
        }
    };

    let mut lines: Vec<String> = Vec::with_capacity(rows.len() * 2);
    for r in &rows {
        lines.push(format!("joined — {}", ts(&r.joined_at)));
        if let Some(left_at) = r.left_at.as_deref() {
            let action = if r.banned { "banned" } else { "left" };
            lines.push(format!("{action} — {}", ts(left_at)));
        }
    }

    let title = format!("History for user {}", uid);
    if lines.is_empty() {
        let embed = serenity::CreateEmbed::new()
            .title(title)
            .description("No membership history found for this user.");

        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        return Ok(());
    }

    send_chunked_embeds(
        ctx,
        lines,
        |first_desc| {
            serenity::CreateEmbed::new()
                .title(title)
                .description(first_desc)
        },
        |index, cont_desc| {
            serenity::CreateEmbed::new()
                .title(format!("History (cont. #{})", index))
                .description(cont_desc)
        },
    )
    .await?;

    Ok(())
}
