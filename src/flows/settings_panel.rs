use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelId, ChannelType, ComponentInteraction, ComponentInteractionDataKind,
    CreateActionRow, CreateButton, CreateSelectMenu, CreateSelectMenuKind, GuildId, UserId,
};

use crate::flows::{ComponentFlow, Surface, UiHandle};
use crate::repos::{GuildSettings, GuildSettingsRepo};
use crate::state::{AppState, Ctx};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftValue {
    Keep,
    Set(ChannelId),
    Clear,
}
impl Default for DraftValue {
    fn default() -> Self {
        DraftValue::Keep
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TempSettings {
    pub join: DraftValue,
    pub leave: DraftValue,
    pub mod_log: DraftValue,
}

pub struct SettingsPanel {
    state: Arc<AppState>,
    guild_id: GuildId,
    author_id: UserId,
    current: GuildSettings,
    draft: TempSettings,
}

impl SettingsPanel {
    pub fn new(
        state: Arc<AppState>,
        guild_id: GuildId,
        author_id: UserId,
        current: GuildSettings,
    ) -> Self {
        Self {
            state,
            guild_id,
            author_id,
            current,
            draft: TempSettings::default(),
        }
    }

    fn render_summary(current: &GuildSettings, draft: &TempSettings) -> String {
        let show = |opt: Option<ChannelId>| {
            opt.map(|c| format!("<#{}>", c.get()))
                .unwrap_or_else(|| "—".into())
        };
        let eff = |cur: Option<ChannelId>, d: DraftValue| match d {
            DraftValue::Keep => cur,
            DraftValue::Set(id) => Some(id),
            DraftValue::Clear => None,
        };
        let j = show(eff(current.join_log, draft.join));
        let l = show(eff(current.leave_log, draft.leave));
        let m = show(eff(current.mod_log, draft.mod_log));
        format!("• Join:  {j}\n• Leave: {l}\n• Mod:   {m}")
    }

    fn build_components(current: &GuildSettings, draft: &TempSettings) -> Vec<CreateActionRow> {
        let eff = |cur: Option<ChannelId>, d: DraftValue| match d {
            DraftValue::Keep => cur,
            DraftValue::Set(id) => Some(id),
            DraftValue::Clear => None,
        };
        let join_default = eff(current.join_log, draft.join).map(|c| vec![c]);
        let leave_default = eff(current.leave_log, draft.leave).map(|c| vec![c]);
        let mod_default = eff(current.mod_log, draft.mod_log).map(|c| vec![c]);

        let join_select = CreateSelectMenu::new(
            "settings_join",
            CreateSelectMenuKind::Channel {
                channel_types: Some(vec![
                    ChannelType::Text,
                    ChannelType::News,
                    ChannelType::Forum,
                ]),
                default_channels: join_default,
            },
        )
        .placeholder("Select Join log channel")
        .min_values(0)
        .max_values(1);

        let leave_select = CreateSelectMenu::new(
            "settings_leave",
            CreateSelectMenuKind::Channel {
                channel_types: Some(vec![
                    ChannelType::Text,
                    ChannelType::News,
                    ChannelType::Forum,
                ]),
                default_channels: leave_default,
            },
        )
        .placeholder("Select Leave log channel")
        .min_values(0)
        .max_values(1);

        let mod_select = CreateSelectMenu::new(
            "settings_mod",
            CreateSelectMenuKind::Channel {
                channel_types: Some(vec![
                    ChannelType::Text,
                    ChannelType::News,
                    ChannelType::Forum,
                ]),
                default_channels: mod_default,
            },
        )
        .placeholder("Select Mod log channel")
        .min_values(0)
        .max_values(1);

        let buttons = vec![
            CreateButton::new("settings_save")
                .label("Save")
                .style(ButtonStyle::Success),
            CreateButton::new("settings_clear")
                .label("Clear All")
                .style(ButtonStyle::Danger),
            CreateButton::new("settings_cancel")
                .label("Cancel")
                .style(ButtonStyle::Secondary),
        ];

        vec![
            serenity::all::CreateActionRow::SelectMenu(join_select),
            serenity::all::CreateActionRow::SelectMenu(leave_select),
            serenity::all::CreateActionRow::SelectMenu(mod_select),
            serenity::all::CreateActionRow::Buttons(buttons),
        ]
    }

    fn handle_select(&mut self, ci: &ComponentInteraction) {
        if let ComponentInteractionDataKind::ChannelSelect { values } = &ci.data.kind {
            let choice = values.first().copied();
            match ci.data.custom_id.as_str() {
                "settings_join" => {
                    self.draft.join = choice.map(DraftValue::Set).unwrap_or(DraftValue::Clear)
                }
                "settings_leave" => {
                    self.draft.leave = choice.map(DraftValue::Set).unwrap_or(DraftValue::Clear)
                }
                "settings_mod" => {
                    self.draft.mod_log = choice.map(DraftValue::Set).unwrap_or(DraftValue::Clear)
                }
                _ => {}
            }
        }
    }

    fn clear_draft(&mut self) {
        self.draft = TempSettings {
            join: DraftValue::Clear,
            leave: DraftValue::Clear,
            mod_log: DraftValue::Clear,
        };
    }

    async fn apply_changes(&self) -> Result<()> {
        let grepo = GuildSettingsRepo::new(&self.state.db);
        grepo.ensure_row(&self.guild_id).await?;

        let apply = |v: DraftValue| -> Option<Option<ChannelId>> {
            match v {
                DraftValue::Keep => None,
                DraftValue::Set(id) => Some(Some(id)),
                DraftValue::Clear => Some(None),
            }
        };

        if let Some(val) = apply(self.draft.join) {
            grepo
                .set_column(&self.guild_id, "join_log_channel_id", val)
                .await?;
        }
        if let Some(val) = apply(self.draft.leave) {
            grepo
                .set_column(&self.guild_id, "leave_log_channel_id", val)
                .await?;
        }
        if let Some(val) = apply(self.draft.mod_log) {
            grepo
                .set_column(&self.guild_id, "mod_log_channel_id", val)
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl ComponentFlow for SettingsPanel {
    fn author_id(&self) -> UserId {
        self.author_id
    }
    fn guild_id(&self) -> GuildId {
        self.guild_id
    }

    async fn on_start(&mut self, ui: &mut UiHandle<'_>, pctx: Option<Ctx<'_>>) -> Result<()> {
        let content = format!(
            "Select channels below, then **Save**.\n\n{}",
            Self::render_summary(&self.current, &self.draft)
        );
        ui.first_render(
            pctx,
            content,
            Self::build_components(&self.current, &self.draft),
        )
        .await
    }

    async fn on_component(
        &mut self,
        ui: &mut UiHandle<'_>,
        ci: &ComponentInteraction,
    ) -> Result<bool> {
        match ci.data.custom_id.as_str() {
            "settings_join" | "settings_leave" | "settings_mod" => {
                self.handle_select(ci);
                let content = format!(
                    "Select channels below, then **Save**.\n\n{}",
                    Self::render_summary(&self.current, &self.draft)
                );
                ui.update_with(
                    ci,
                    content,
                    Self::build_components(&self.current, &self.draft),
                )
                .await?;
                Ok(true)
            }
            "settings_clear" => {
                self.clear_draft();
                let content = format!(
                    "Select channels below, then **Save**.\n\n{}",
                    Self::render_summary(&self.current, &self.draft)
                );
                ui.reset_with(
                    ci,
                    content,
                    Self::build_components(&self.current, &self.draft),
                )
                .await?;
                Ok(true)
            }
            "settings_cancel" => {
                ui.finish_with(ci, "Cancelled. No changes were saved.".into())
                    .await?;
                Ok(false)
            }
            "settings_save" => {
                self.apply_changes().await?;
                // refresh for final summary
                let grepo = GuildSettingsRepo::new(&self.state.db);
                self.current = grepo.get(&self.guild_id).await?;
                let final_content = format!(
                    "Saved ✅\n\n{}",
                    Self::render_summary(&self.current, &TempSettings::default())
                );
                ui.finish_with(ci, final_content).await?;
                Ok(false)
            }
            _ => Ok(true),
        }
    }
}
