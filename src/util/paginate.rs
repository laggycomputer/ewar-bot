use crate::util::base_embed;
use crate::{BotError, Context};
use poise::CreateReply;
use serenity::all::{CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, CreateInteractionResponseMessage, ReactionType};
use serenity::builder::CreateInteractionResponse;
use std::time::Duration;

pub(crate) struct EmbedLinePaginator {
    pages: Vec<String>,
    current_page: u8,
}

impl EmbedLinePaginator {
    pub(crate) fn new(lines: Vec<String>) -> EmbedLinePaginator {
        let mut chunks = Vec::new();
        chunks.push(String::new());
        let mut working_chunk = &mut chunks[0];
        let mut num_chunks = 1;

        for line in lines {
            if working_chunk.chars().count() + 1 + line.chars().count() > 4096 {
                chunks.push(String::new());
                num_chunks += 1;
                working_chunk = &mut chunks[num_chunks - 1];
            }

            working_chunk.push('\n');
            working_chunk.push_str(&*line);
        }

        Self { pages: chunks, current_page: 1 }
    }

    fn embed_for(&self, ctx: Context<'_>, page: u8) -> CreateEmbed {
        base_embed(ctx)
            .description(self.pages[(page - 1) as usize].clone())
            .footer(CreateEmbedFooter::new(format!("{page}/{}", self.pages.len())))
    }

    pub(crate) async fn run(mut self, ctx: Context<'_>) -> Result<(), BotError> {
        let components = if self.pages.len() > 1 {
            vec![CreateActionRow::Buttons(vec![
                CreateButton::new("embedinator_start")
                    .emoji(ReactionType::Unicode("⏮️".parse().unwrap())),
                CreateButton::new("embedinator_previous")
                    .emoji(ReactionType::Unicode("◀️".parse().unwrap())),
                CreateButton::new("embedinator_next")
                    .emoji(ReactionType::Unicode("▶️".parse().unwrap())),
                CreateButton::new("embedinator_end")
                    .emoji(ReactionType::Unicode("⏭️".parse().unwrap())),
                CreateButton::new("embedinator_stop")
                    .emoji(ReactionType::Unicode("⏹️".parse().unwrap())),
            ])]
        } else {
            vec![]
        };

        let sent_handle = ctx.send(CreateReply::default()
            .embed(base_embed(ctx))
            .reply(true)
            .components(components)).await?;

        if self.pages.len() <= 1 {
            return Ok(());
        }

        let sent_message = sent_handle.message().await?;
        while let Some(ixn) = sent_message.await_component_interaction(&ctx.serenity_context().shard)
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(30)).await {
            match ixn.data.custom_id.as_str() {
                "embedinator_start" => {
                    self.current_page = 1;
                    ixn.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(self.embed_for(ctx, self.current_page)))).await?;
                }
                "embedinator_previous" => {
                    self.current_page -= 1;
                    if self.current_page == 0 { self.current_page = self.pages.len() as u8 }
                    ixn.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(self.embed_for(ctx, self.current_page)))).await?;
                }
                "embedinator_next" => {
                    self.current_page += 1;
                    if self.current_page >= self.pages.len() as u8 { self.current_page = 0 }
                    ixn.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(self.embed_for(ctx, self.current_page)))).await?;
                }
                "embedinator_end" => {
                    self.current_page = self.pages.len() as u8;
                    ixn.create_response(ctx.http(), CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(self.embed_for(ctx, self.current_page)))).await?;
                }
                "embedinator_stop" => break,
                _ => {}
            }
        };

        sent_handle.edit(ctx, CreateReply::default()
            .embed(self.embed_for(ctx, self.current_page))
            .components(vec![])).await?;

        Ok(())
    }
}