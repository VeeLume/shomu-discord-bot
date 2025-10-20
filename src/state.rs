use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use serenity::all::{GuildId, UserId};

use crate::db::Db;

pub use crate::repos::GuildSettings;
pub type Ctx<'a> = poise::Context<'a, std::sync::Arc<AppState>, anyhow::Error>;

/// AppState: holds Db and all in-memory caches.
/// No SQL here; only quick state helpers.
pub struct AppState {
    pub db: Db,

    /// invite_cache[guild_id][code] = uses
    pub invite_cache: DashMap<GuildId, HashMap<String, u64>>,

    /// Recent bans for leave classification
    pub recent_bans: DashMap<GuildId, DashMap<UserId, i64>>,
}

impl AppState {
    pub async fn new(db_url: &str) -> Result<Arc<Self>, anyhow::Error> {
        let db = crate::db::Db::connect(db_url).await?;
        Ok(Arc::new(Self {
            db,
            invite_cache: DashMap::new(),
            recent_bans: DashMap::new(),
        }))
    }

    pub fn mark_recent_ban(&self, guild_id: GuildId, user_id: UserId) {
        let now = unix_now();
        let m = self
            .recent_bans
            .entry(guild_id)
            .or_insert_with(DashMap::new);
        m.insert(user_id, now);
    }

    pub fn was_recently_banned(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        window_secs: i64,
    ) -> bool {
        if let Some(map) = self.recent_bans.get(&guild_id) {
            if let Some(ts) = map.get(&user_id) {
                return unix_now() - *ts <= window_secs;
            }
        }
        false
    }

    pub fn prune_recent_bans(&self, max_age_secs: i64) {
        let now = unix_now();
        for gmap in self.recent_bans.iter_mut() {
            let to_remove: Vec<UserId> = gmap
                .iter()
                .filter_map(|kv| {
                    let (uid, ts) = (kv.key().to_owned(), *kv.value());
                    if now - ts > max_age_secs {
                        Some(uid)
                    } else {
                        None
                    }
                })
                .collect();
            for uid in to_remove {
                gmap.remove(&uid);
            }
        }
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
