use anyhow::Result;
use poise::serenity_prelude as serenity;
use serenity::all::ChannelId;

use crate::db::Db;

#[derive(Debug, Clone, Copy, Default)]
pub struct GuildSettings {
    pub join_log: Option<ChannelId>,
    pub leave_log: Option<ChannelId>,
    pub mod_log: Option<ChannelId>,
}

#[derive(Clone)]
pub struct GuildSettingsRepo<'a> {
    db: &'a Db,
}

impl<'a> GuildSettingsRepo<'a> {
    pub fn new(db: &'a Db) -> Self { Self { db } }

    pub async fn get(&self, guild_id: &serenity::all::GuildId) -> Result<GuildSettings> {
        let guild = guild_id.to_string();
        let rec = sqlx::query!(
            r#"
            SELECT join_log_channel_id, leave_log_channel_id, mod_log_channel_id
            FROM guild_settings WHERE guild_id = ?
            "#,
            guild
        )
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(GuildSettings {
            join_log: rec
                .as_ref()
                .and_then(|r| r.join_log_channel_id.as_deref())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::all::ChannelId::new),
            leave_log: rec
                .as_ref()
                .and_then(|r| r.leave_log_channel_id.as_deref())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::all::ChannelId::new),
            mod_log: rec
                .as_ref()
                .and_then(|r| r.mod_log_channel_id.as_deref())
                .and_then(|s| s.parse::<u64>().ok())
                .map(serenity::all::ChannelId::new),
        })
    }

    pub async fn upsert(
        &self,
        guild_id: &serenity::all::GuildId,
        join: Option<ChannelId>,
        leave: Option<ChannelId>,
        log_channel: Option<ChannelId>,
    ) -> Result<()> {
        let guild_id = guild_id.to_string();
        let join = join.map(|c| c.to_string());
        let leave = leave.map(|c| c.to_string());
        let modu  = log_channel.map(|c| c.to_string());

        sqlx::query!(
            r#"
            INSERT INTO guild_settings (guild_id, join_log_channel_id, leave_log_channel_id, mod_log_channel_id)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(guild_id) DO UPDATE SET
              join_log_channel_id = COALESCE(excluded.join_log_channel_id, guild_settings.join_log_channel_id),
              leave_log_channel_id = COALESCE(excluded.leave_log_channel_id, guild_settings.leave_log_channel_id),
              mod_log_channel_id   = COALESCE(excluded.mod_log_channel_id,   guild_settings.mod_log_channel_id)
            "#,
            guild_id, join, leave, modu
        )
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }

    /// Ensure row exists (used before column-wise updates).
    pub async fn ensure_row(&self, guild_id: &serenity::all::GuildId) -> Result<()> {
        let gid = guild_id.to_string();
        sqlx::query!(
            r#"INSERT INTO guild_settings (guild_id) VALUES (?) ON CONFLICT(guild_id) DO NOTHING"#,
            gid
        ).execute(&self.db.pool).await?;
        Ok(())
    }

    pub async fn set_column(
        &self,
        guild_id: &serenity::all::GuildId,
        column: &str,
        value: Option<ChannelId>,
    ) -> Result<()> {
        let gid = guild_id.to_string();
        if let Some(id) = value {
            let q = format!("UPDATE guild_settings SET {column} = ? WHERE guild_id = ?");
            sqlx::query(&q).bind(id.get().to_string()).bind(gid).execute(&self.db.pool).await?;
        } else {
            let q = format!("UPDATE guild_settings SET {column} = NULL WHERE guild_id = ?");
            sqlx::query(&q).bind(gid).execute(&self.db.pool).await?;
        }
        Ok(())
    }
}
