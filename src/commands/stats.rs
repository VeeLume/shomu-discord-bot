use anyhow::Result;
use chrono::Datelike;
use poise::serenity_prelude as serenity;

use crate::repos::MembershipsRepo;
use crate::state::Ctx;

/// Top users who rejoined (had multiple stints).
#[poise::command(slash_command, guild_only)]
pub async fn stats_rejoiners(
    ctx: Ctx<'_>,
    #[description = "Minimum rejoins (default 2)"] min_rejoins: Option<i64>,
    #[description = "Max users to show (default 15)"] limit: Option<i64>,
) -> Result<()> {
    let gid = match ctx.guild_id() {
        Some(g) => g,
        None => {
            ctx.say("Guild-only").await?;
            return Ok(());
        }
    };

    let min_rejoins = min_rejoins.unwrap_or(2).max(2);
    let limit = limit.unwrap_or(15).clamp(1, 100);

    let repo = MembershipsRepo::new(&ctx.data().db);
    let rows = repo.rejoiners(gid, min_rejoins, limit).await?;

    if rows.is_empty() {
        ctx.say(format!("No users with ≥{} rejoins.", min_rejoins))
            .await?;
        return Ok(());
    }

    let mut lines = Vec::with_capacity(rows.len());
    for r in rows {
        let label = match (r.server_username.as_deref(), r.account_username.as_deref()) {
            (Some(nick), Some(acc)) if !nick.is_empty() => format!("{nick} (aka {acc})"),
            (_, Some(acc)) => acc.to_string(),
            (Some(nick), None) => nick.to_string(),
            _ => format!("<@{}>", r.user_id),
        };
        lines.push(format!(
            "• {label} — {} rejoins ({} exits)",
            r.rejoin_count, r.times_left
        ));
    }

    let embed = serenity::CreateEmbed::new()
        .title(format!("Rejoiners (≥{} rejoins)", min_rejoins))
        .description(lines.join("\n"));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Recent exits with left vs banned split.
#[poise::command(slash_command, guild_only)]
pub async fn stats_exits(
    ctx: Ctx<'_>,
    #[description = "Look back this many days (default 30)"] days: Option<i64>,
    #[description = "Max rows shown (default 20)"] show: Option<i64>,
) -> Result<()> {
    use chrono::{DateTime, Duration, Utc};

    let gid = match ctx.guild_id() {
        Some(g) => g,
        None => {
            ctx.say("Guild-only").await?;
            return Ok(());
        }
    };

    let days = days.unwrap_or(30).clamp(1, 365);
    let show = show.unwrap_or(20).clamp(1, 100);

    // Pull a safety window: get up to 2k exits and filter in Rust by timestamp
    let repo = MembershipsRepo::new(&ctx.data().db);
    let rows = repo.all_exits(gid, 2000).await?;

    let now = Utc::now();
    let cutoff = now - Duration::days(days);

    let mut filtered = Vec::new();
    let mut left_count = 0usize;
    let mut banned_count = 0usize;

    for r in rows {
        // Parse RFC2822
        if let Ok(dt) = DateTime::parse_from_rfc2822(&r.left_at) {
            let dt_utc = dt.with_timezone(&Utc);
            if dt_utc >= cutoff {
                if r.banned {
                    banned_count += 1;
                } else {
                    left_count += 1;
                }
                filtered.push((dt_utc, r));
            }
        }
    }

    if filtered.is_empty() {
        ctx.say(format!("No exits in the last {} days.", days))
            .await?;
        return Ok(());
    }

    // Sort newest first
    filtered.sort_by_key(|(t, _)| *t);
    filtered.reverse();

    let total = left_count + banned_count;
    let mut lines = Vec::new();
    lines.push(format!(
        "**Total:** {} (left: {}, banned: {})",
        total, left_count, banned_count
    ));
    lines.push("".into());

    for (_, r) in filtered.iter().take(show as usize) {
        let label = match (r.server_username.as_deref(), r.account_username.as_deref()) {
            (Some(nick), Some(acc)) if !nick.is_empty() => format!("{nick} (aka {acc})"),
            (_, Some(acc)) => acc.to_string(),
            (Some(nick), None) => nick.to_string(),
            _ => format!("<@{}>", r.user_id),
        };

        // Discord timestamp token
        let ts = if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(&r.left_at) {
            format!("<t:{}:R>", dt.timestamp())
        } else {
            r.left_at.clone()
        };

        let kind = if r.banned { "**banned**" } else { "left" };
        lines.push(format!("• {label} — {kind} — {ts}"));
    }

    let embed = serenity::CreateEmbed::new()
        .title(format!("Exits in last {} days", days))
        .description(lines.join("\n"));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Snapshot counts: current members, lifetime uniques, exits, bans, stints.
#[poise::command(slash_command, guild_only)]
pub async fn stats_current(ctx: Ctx<'_>) -> Result<()> {
    let gid = match ctx.guild_id() {
        Some(g) => g,
        None => {
            ctx.say("Guild-only").await?;
            return Ok(());
        }
    };

    let repo = MembershipsRepo::new(&ctx.data().db);
    let s = repo.stats_current(gid).await?;

    let embed = serenity::CreateEmbed::new()
        .title("Current stats")
        .field(
            "Current members",
            format!("**{}**", s.current_members),
            true,
        )
        .field("Unique users ever", format!("{}", s.unique_ever), true)
        .field("Total rejoins", format!("{}", s.total_rejoins), true)
        .field("Total exits", format!("{}", s.total_exits), true)
        .field("Banned (of exits)", format!("{}", s.total_banned), true)
        .field(
            "Left (of exits)",
            format!("{}", s.total_exits.saturating_sub(s.total_banned)),
            true,
        );

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Daily net member delta (joins - leaves) with totals and unique users.
#[poise::command(slash_command, guild_only)]
pub async fn stats_member_balance(
    ctx: Ctx<'_>,
    #[description = "Days to look back (default 30)"] days: Option<i64>,
    #[description = "Max rows to scan (default 2000)"] cap: Option<i64>,
) -> anyhow::Result<()> {
    use chrono::{DateTime, Duration, NaiveDate, Utc};
    use std::collections::{BTreeMap, BTreeSet};

    let gid = match ctx.guild_id() {
        Some(g) => g,
        None => {
            ctx.say("Guild-only").await?;
            return Ok(());
        }
    };

    let days = days.unwrap_or(30).clamp(1, 365);
    let cap = cap.unwrap_or(2000).clamp(100, 100_000);

    let repo = MembershipsRepo::new(&ctx.data().db);
    let raw = repo.recent_rejoins_raw(gid, cap).await?;

    let cutoff = Utc::now() - Duration::days(days);

    // Per-day tallies
    struct Tallies {
        total: i64,
        uniq: BTreeSet<String>,
    }
    impl Default for Tallies {
        fn default() -> Self {
            Self {
                total: 0,
                uniq: BTreeSet::new(),
            }
        }
    }

    let mut joins: BTreeMap<NaiveDate, Tallies> = BTreeMap::new();
    let mut leaves: BTreeMap<NaiveDate, Tallies> = BTreeMap::new();

    for item in raw {
        // joins
        if let Ok(jdt) = DateTime::parse_from_rfc2822(&item.joined_at) {
            let jutc = jdt.with_timezone(&Utc);
            if jutc >= cutoff {
                let d = jutc.date_naive();
                let e = joins.entry(d).or_default();
                e.total += 1;
                e.uniq.insert(item.user_id.clone());
            }
        }
        // leaves
        if let Some(left) = &item.left_at {
            if let Ok(ldt) = DateTime::parse_from_rfc2822(left) {
                let lutc = ldt.with_timezone(&Utc);
                if lutc >= cutoff {
                    let d = lutc.date_naive();
                    let e = leaves.entry(d).or_default();
                    e.total += 1;
                    e.uniq.insert(item.user_id.clone());
                }
            }
        }
    }

    // union of all days present
    let all_days: BTreeSet<_> = joins.keys().chain(leaves.keys()).copied().collect();
    if all_days.is_empty() {
        ctx.say(format!("No join/leave activity in the last {} days.", days))
            .await?;
        return Ok(());
    }

    // header totals (window-wide)
    let (mut j_total, mut j_uniq_all) = (0i64, BTreeSet::<String>::new());
    let (mut l_total, mut l_uniq_all) = (0i64, BTreeSet::<String>::new());

    for (_d, t) in &joins {
        j_total += t.total;
        j_uniq_all.extend(t.uniq.iter().cloned());
    }
    for (_d, t) in &leaves {
        l_total += t.total;
        l_uniq_all.extend(t.uniq.iter().cloned());
    }

    let net_total = j_total - l_total;

    // lines per day (chronological)
    let mut lines = Vec::new();
    lines.push(format!(
        "**Window totals ({} days):**  net {:+}  |  joins: {} ({} unique)  |  leaves: {} ({} unique)",
        days, net_total, j_total, j_uniq_all.len(), l_total, l_uniq_all.len()
    ));
    lines.push("".into());

    for d in all_days {
        let j = joins.get(&d);
        let l = leaves.get(&d);

        let jt = j.map(|x| x.total).unwrap_or(0);
        let ju = j.map(|x| x.uniq.len()).unwrap_or(0);
        let lt = l.map(|x| x.total).unwrap_or(0);
        let lu = l.map(|x| x.uniq.len()).unwrap_or(0);
        let net = jt - lt;

        let sign = if net > 0 {
            "+"
        } else if net < 0 {
            "−"
        } else {
            " "
        };
        lines.push(format!(
            "{d}  {sign}{:>2}  (joins: {} / {} unique,  leaves: {} / {} unique)",
            net.abs(),
            jt,
            ju,
            lt,
            lu
        ));
    }

    let embed = serenity::CreateEmbed::new()
        .title(format!("Member balance (last {} days)", days))
        .description(lines.join("\n"));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
