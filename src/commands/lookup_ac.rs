use anyhow::Result;
use poise::serenity_prelude as serenity;

use crate::repos::MembershipsRepo;
use crate::state::Ctx;

/// Autocomplete by nickname/account username; returns `AutocompleteChoice<label, value=user_id>`
pub async fn ac_member(
    ctx: Ctx<'_>,
    partial: &str,
) -> Vec<serenity::AutocompleteChoice> {
    let Some(gid) = ctx.guild_id() else { return Vec::new(); };

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

/// Show the membership history for a user picked via autocomplete.
#[poise::command(slash_command, guild_only)]
pub async fn userinfo_lookup(
    ctx: Ctx<'_>,
    #[description = "Pick a user by name"]
    #[autocomplete = "ac_member"]
    user_id: String,
) -> Result<()> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Guild-only").await?;
        return Ok(());
    };

    // Load history and render (same shape as your existing /userinfo)
    let repo = MembershipsRepo::new(&ctx.data().db);
    let uid_num = user_id.parse::<u64>().ok().map(serenity::all::UserId::new);

    let rows = if let Some(uid) = uid_num {
        repo.history_for_user(guild_id, uid).await?
    } else {
        // Shouldn't happen because autocomplete provides IDs, but guard it.
        ctx.say("Couldn't parse that user id.").await?;
        return Ok(());
    };

    // Build lines
    let ts = |rfc2822: &str| -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(rfc2822) {
            format!("<t:{}:f>", dt.timestamp())
        } else { rfc2822.to_string() }
    };

    let mut lines = Vec::with_capacity(rows.len() * 2);
    for r in &rows {
        lines.push(format!("joined — {}", ts(&r.joined_at)));
        if let Some(left_at) = r.left_at.as_deref() {
            let action = if r.banned { "banned" } else { "left" };
            lines.push(format!("{action} — {}", ts(left_at)));
        }
    }

    // Title prefers live user tag
    let mut title = format!("History for <@{}>", user_id);
    if let Some(uid) = uid_num {
        if let Ok(user) = ctx.serenity_context().http.get_user(uid).await {
            title = format!("History for {}", user.tag());
        }
    }

    let embed = serenity::CreateEmbed::new()
        .title(title)
        .description(if rows.is_empty() { "No history found.".into() } else { lines.join("\n") });

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
