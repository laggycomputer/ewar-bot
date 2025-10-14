use crate::util::base_embed;
use crate::{BotError, Context};
use poise::CreateReply;
use serenity::all::{CreateActionRow, CreateButton, CreateEmbed, CreateEmbedFooter, CreateInteractionResponseMessage, ReactionType};
use serenity::builder::CreateInteractionResponse;
use std::cmp::min;
use std::num::NonZeroUsize;
use std::time::Duration;

pub(crate) struct EmbedLinePaginator {
    pages: Vec<String>,
    current_page: u8,
}

pub(crate) struct PaginatorOptions {
    sep: Box<str>,
    max_lines: Option<usize>,
    // paginator will default to and cap at 4096
    char_limit: usize,
}

impl Default for PaginatorOptions {
    fn default() -> Self {
        Self {
            sep: Box::from("\n"),
            max_lines: None,
            char_limit: 4096,
        }
    }
}

impl PaginatorOptions {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sep(mut self, sep: impl Into<Box<str>>) -> Self {
        self.sep = sep.into();
        self
    }

    pub(crate) fn max_lines(mut self, max_lines: impl Into<NonZeroUsize>) -> Self {
        self.max_lines = Some(max_lines.into().get());
        self
    }

    pub(crate) fn char_limit(mut self, char_limit: impl Into<NonZeroUsize>) -> Self {
        self.char_limit = min(char_limit.into().get(), 4096);
        self
    }
}

impl EmbedLinePaginator {
    pub(crate) fn new(lines: Vec<Box<str>>, options: PaginatorOptions) -> EmbedLinePaginator {
        let mut chunks = Vec::new();
        chunks.push(String::new());
        let mut working_chunk = &mut chunks[0];
        let mut num_in_working_chunk = 0usize;
        let mut num_chunks = 1usize;

        for line in lines {
            if working_chunk.chars().count() + options.sep.len() + line.chars().count() > options.char_limit
                || num_in_working_chunk >= options.max_lines.unwrap_or(usize::MAX) {
                chunks.push(String::new());
                num_chunks += 1;
                working_chunk = &mut chunks[num_chunks - 1];
                num_in_working_chunk = 0;
            }

            working_chunk.push_str(&*options.sep);
            working_chunk.push_str(&*line);
            num_in_working_chunk += 1;
        }

        Self {
            pages: chunks,
            current_page: 1,
        }
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
                    .emoji(ReactionType::Unicode(String::from("⏮️"))),
                CreateButton::new("embedinator_previous")
                    .emoji(ReactionType::Unicode(String::from("◀️"))),
                CreateButton::new("embedinator_next")
                    .emoji(ReactionType::Unicode(String::from("▶️"))),
                CreateButton::new("embedinator_end")
                    .emoji(ReactionType::Unicode(String::from("⏭️"))),
                CreateButton::new("embedinator_stop")
                    .emoji(ReactionType::Unicode(String::from("⏹️"))),
            ])]
        } else {
            vec![]
        };

        let sent_handle = ctx.send(CreateReply::default()
            .embed(self.embed_for(ctx, self.current_page))
            .reply(true)
            .components(components)).await?;

        if self.pages.len() <= 1 {
            return Ok(());
        }

        let sent_message = sent_handle.message().await?;
        loop {
            if let Some(ixn) = sent_message.await_component_interaction(&ctx.serenity_context().shard)
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
                        if self.current_page > self.pages.len() as u8 { self.current_page = 1 }
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
            } else {
                break;
            }
        }

        sent_handle.edit(ctx, CreateReply::default()
            .embed(self.embed_for(ctx, self.current_page))
            .components(vec![])).await?;

        Ok(())
    }
}