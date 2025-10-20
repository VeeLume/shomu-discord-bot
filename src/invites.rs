use std::collections::HashMap;

use anyhow::Result;
use serenity::all::{GuildId, RichInvite};
use serenity::http::Http;

/// Fetch all invites for a guild (requires Manage Guild) and map code->uses.
pub async fn fetch_invites_map(http: &Http, guild_id: GuildId) -> Result<HashMap<String, u64>> {
    let invites: Vec<RichInvite> = guild_id.invites(http).await?;
    Ok(invites
        .into_iter()
        .filter_map(|i| Some((i.code, i.uses)))
        .collect())
}
