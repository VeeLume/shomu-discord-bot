use anyhow::Result;
use poise::serenity_prelude as serenity;

use crate::repos::MembershipsRepo;
use crate::state::Ctx;

#[poise::command(slash_command, context_menu_command = "User information", guild_only)]
pub async fn userinfo(
    ctx: Ctx<'_>,
    #[description = "User to look up"] user: serenity::User,
) -> Result<()> {
    let guild_id = match ctx.guild_id() {
        Some(gid) => gid,
        None => {
            ctx.say("This command can only be used in a guild.").await?;
            return Ok(());
        }
    };

    let mrepo = MembershipsRepo::new(&ctx.data().db);
    let rows = mrepo.history_for_user(guild_id, user.id).await?;

    if rows.is_empty() {
        ctx.say(format!("No membership history found for {}.", user.tag()))
            .await?;
        return Ok(());
    }

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

    let embed = serenity::CreateEmbed::new()
        .title(format!("History for {}", user.tag()))
        .thumbnail(user.face())
        .description(lines.join("\n"));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
