use anyhow::Result;
use poise::serenity_prelude as serenity;

use crate::commands::send_chunked_embeds;
use crate::repos::MembershipsRepo;
use crate::state::Ctx;

/// Slash + context menu for user info / history.
///
/// - Slash: `/userinfo user:<pick member>`
/// - Context menu: right click user → "User information"
#[poise::command(
    slash_command,
    context_menu_command = "User information",
    guild_only,
    ephemeral
)]
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

    // Helper to format timestamps as Discord timestamps
    let ts = |rfc2822: &str| -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(rfc2822) {
            format!("<t:{}:f>", dt.timestamp())
        } else {
            rfc2822.to_string()
        }
    };

    let title = format!("History for {}", user.tag());
    let thumb_url = user.face();

    if rows.is_empty() {
        let embed = serenity::CreateEmbed::new()
            .title(title)
            .thumbnail(thumb_url)
            .description("No server stays recorded for this user.");

        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        return Ok(());
    }

    // Build history lines for all stays
    let mut lines: Vec<String> = Vec::with_capacity(rows.len() * 2);
    for r in &rows {
        lines.push(format!("joined — {}", ts(&r.joined_at)));
        if let Some(left_at) = r.left_at.as_deref() {
            let action = if r.banned { "banned" } else { "left" };
            lines.push(format!("{action} — {}", ts(left_at)));
        }
    }

    let stay_count = rows.len();
    let last = rows.last().unwrap();
    let currently_in_guild = last.left_at.is_none();

    let status_line = if currently_in_guild {
        format!(
            "Currently **in** the server (last joined: {}).",
            ts(&last.joined_at)
        )
    } else if let Some(left) = last.left_at.as_deref() {
        format!("Last seen in server: {}.", ts(left))
    } else {
        "Status unknown.".to_string()
    };

    let base_title = title.clone();
    let base_title_cont = base_title.clone();
    let thumb_url_first = thumb_url.clone();
    let status_line_first = status_line.clone();
    let stay_count_first = stay_count.to_string();

    // Use the generic helper, but customize the first embed heavily.
    send_chunked_embeds(
        ctx,
        lines,
        move |desc| {
            serenity::CreateEmbed::new()
                .title(base_title.clone())
                .thumbnail(thumb_url_first.clone())
                .field("Server stays", stay_count_first.clone(), true)
                .field("Current status", status_line_first.clone(), false)
                .description(desc)
        },
        move |idx, desc| {
            serenity::CreateEmbed::new()
                .title(format!("{base_title_cont} — cont. #{idx}"))
                .description(desc)
        },
    )
    .await?;

    Ok(())
}
