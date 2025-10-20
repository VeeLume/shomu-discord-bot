use ::serenity::futures::StreamExt;
use anyhow::Result;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;

pub mod settings_panel;
/// Where the UI lives.
#[derive(Debug, Clone, Copy)]
pub enum Surface {
    /// Respond to the interaction (can be ephemeral). We keep editing that response.
    AttachedEphemeral,
    /// Send a standalone message in a channel and edit that.
    DetachedMessage {
        channel_id: serenity::all::ChannelId,
    },
}

/// Lightweight UI handle your flow uses to render/update/finish/reset.
pub struct UiHandle<'a> {
    pub sctx: &'a serenity::prelude::Context,
    surface: Surface,
    message_id: Option<serenity::all::MessageId>,
    delete_on_finish: bool,
}

impl<'a> UiHandle<'a> {
    fn new(sctx: &'a serenity::prelude::Context, surface: Surface) -> Self {
        Self {
            sctx,
            surface,
            message_id: None,
            delete_on_finish: true,
        }
    }
    fn set_message_id(&mut self, id: serenity::all::MessageId) {
        self.message_id = Some(id);
    }
    pub fn message_id(&self) -> Option<serenity::all::MessageId> {
        self.message_id
    }
    pub fn delete_on_finish(mut self, yes: bool) -> Self {
        self.delete_on_finish = yes;
        self
    }

    /// First render: reply (attached) or send message (detached).
    pub async fn first_render(
        &mut self,
        pctx: Option<crate::state::Ctx<'_>>,
        content: String,
        components: Vec<serenity::all::CreateActionRow>,
    ) -> Result<()> {
        match self.surface {
            Surface::AttachedEphemeral => {
                let pctx = pctx.expect("AttachedEphemeral requires poise::Context");
                pctx.send(
                    poise::CreateReply::default()
                        .content(content)
                        .ephemeral(true)
                        .components(components),
                )
                .await?;
            }
            Surface::DetachedMessage { channel_id } => {
                let msg = channel_id
                    .send_message(
                        self.sctx,
                        serenity::all::CreateMessage::new()
                            .content(content)
                            .components(components),
                    )
                    .await?;
                self.set_message_id(msg.id);
            }
        }
        Ok(())
    }

    /// Acknowledge and the interaction
    pub async fn acknowledge(&self, ci: &serenity::all::ComponentInteraction) -> Result<()> {
        ci.create_response(
            self.sctx,
            serenity::all::CreateInteractionResponse::Acknowledge,
        )
        .await?;
        Ok(())
    }

    /// Update the same surface using the interaction token.
    pub async fn update_with(
        &self,
        ci: &serenity::all::ComponentInteraction,
        content: String,
        components: Vec<serenity::all::CreateActionRow>,
    ) -> Result<()> {
        ci.create_response(
            self.sctx,
            serenity::all::CreateInteractionResponse::UpdateMessage(
                serenity::all::CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(components),
            ),
        )
        .await?;
        Ok(())
    }

    /// **Reset** the UI by ACK → DELETE → RE-RENDER brand new.
    /// - Attached: delete the *interaction response* and send an ephemeral followup.
    /// - Detached: delete the existing message and send a new one in the same channel.
    pub async fn reset_with(
        &mut self,
        ci: &serenity::all::ComponentInteraction,
        content: String,
        components: Vec<serenity::all::CreateActionRow>,
    ) -> Result<()> {
        use serenity::all::{CreateInteractionResponse, CreateInteractionResponseFollowup};

        // 1) ACK the click without editing
        ci.create_response(self.sctx, CreateInteractionResponse::Acknowledge)
            .await
            .ok();

        match self.surface {
            Surface::AttachedEphemeral => {
                // 2) Delete the original interaction response (drops client state)
                let _ = ci.delete_response(self.sctx).await;
                // 3) Send a brand-new ephemeral followup
                ci.create_followup(
                    self.sctx,
                    CreateInteractionResponseFollowup::new()
                        .ephemeral(true)
                        .content(content)
                        .components(components),
                )
                .await?;
                // No message_id for attached/ephemeral – collector doesn’t filter by it anyway.
            }
            Surface::DetachedMessage { channel_id } => {
                // 2) Delete our previous message if we had one
                if let Some(mid) = self.message_id {
                    let _ = channel_id.delete_message(self.sctx, mid).await;
                    // 3) Send a completely new message
                    let msg = channel_id
                        .send_message(
                            self.sctx,
                            serenity::all::CreateMessage::new()
                                .content(content)
                                .components(components),
                        )
                        .await?;
                    // 4) Update message_id so subsequent updates target the new message
                    self.set_message_id(msg.id);
                } else {
                    // No prior message tracked (shouldn't happen), fallback to a fresh send
                    let msg = channel_id
                        .send_message(
                            self.sctx,
                            serenity::all::CreateMessage::new()
                                .content(content)
                                .components(components),
                        )
                        .await?;
                    self.set_message_id(msg.id);
                }
            }
        }
        Ok(())
    }

    /// Finish the flow by replacing UI with final content (no components).
    pub async fn finish_with(
        &self,
        ci: &serenity::all::ComponentInteraction,
        content: String,
    ) -> Result<()> {
        ci.create_response(
            self.sctx,
            serenity::all::CreateInteractionResponse::UpdateMessage(
                serenity::all::CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(vec![]),
            ),
        )
        .await?;
        Ok(())
    }

    /// Cleanup: if detached and we created a message, delete it (unless disabled).
    pub async fn cleanup(&self) {
        if !self.delete_on_finish {
            return;
        }
        if let Surface::DetachedMessage { channel_id } = self.surface {
            if let Some(mid) = self.message_id {
                let _ = channel_id.delete_message(self.sctx, mid).await;
            }
        }
    }
}

#[async_trait]
pub trait ComponentFlow: Send {
    fn author_id(&self) -> serenity::all::UserId;
    fn guild_id(&self) -> serenity::all::GuildId;

    async fn on_start(
        &mut self,
        ui: &mut UiHandle<'_>,
        pctx: Option<crate::state::Ctx<'_>>,
    ) -> Result<()>;

    /// Return Ok(true) to continue; Ok(false) to finish.
    async fn on_component(
        &mut self,
        ui: &mut UiHandle<'_>,
        ci: &serenity::all::ComponentInteraction,
    ) -> Result<bool>;
}

/// Runner. `filter_by_message_id` should be:
/// - true: safer for detached flows that *won't* reset
/// - false: use when a detached flow *can* recreate its message (so clicks on the new message are accepted)
pub async fn run<F: ComponentFlow>(
    sctx: &serenity::prelude::Context,
    surface: Surface,
    mut flow: F,
    pctx_if_attached: Option<crate::state::Ctx<'_>>,
    timeout_secs: u64,
    filter_by_message_id: bool,
) -> Result<()> {
    use serenity::all::ComponentInteractionCollector;

    let owner = flow.author_id();
    let gid = flow.guild_id();
    let mut ui = UiHandle::new(sctx, surface);

    // Initial render
    flow.on_start(&mut ui, pctx_if_attached).await?;

    // Collector scoped by author+guild and, optionally, by message_id.
    let mut col = {
        let mut c = ComponentInteractionCollector::new(sctx)
            .author_id(owner)
            .guild_id(gid)
            .timeout(std::time::Duration::from_secs(timeout_secs));

        if filter_by_message_id {
            if let Some(mid) = ui.message_id() {
                c = c.message_id(mid);
            }
        }
        c
    }
    .stream();

    while let Some(ci) = col.next().await {
        let keep_going = flow.on_component(&mut ui, &ci).await?;
        if !keep_going {
            ui.cleanup().await;
            return Ok(());
        }
    }

    ui.cleanup().await;
    Ok(())
}
