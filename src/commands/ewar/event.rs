use std::num::NonZeroUsize;
use crate::model::{EventNumber, StandingEvent};
use crate::util::paginate::{EmbedLinePaginator, PaginatorOptions};
use crate::{BotError, Context};
use bson::doc;
use futures::TryStreamExt;
use crate::util::constants::LOG_LIMIT;

#[poise::command(prefix_command, slash_command, subcommands("log"))]
pub(crate) async fn event(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}

/// See past events
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn log(
    ctx: Context<'_>,
    #[description = "skip events after this event number"] before: Option<EventNumber>,
) -> Result<(), BotError> {
    ctx.defer().await?;

    let filter_doc = if before.is_some() {
        doc! { "_id": { "$lte": before.unwrap() } }
    } else {
        doc! {}
    };

    let mut lines = Vec::new();
    let mut cur = ctx.data().mongo.collection::<StandingEvent>("events")
        .find(filter_doc)
        .sort(doc! { "_id": -1 })
        .limit(LOG_LIMIT)
        .await?;
    while let Some(event) = cur.try_next().await? { lines.push(event.short_summary(&ctx.data().mongo).await?) }

    EmbedLinePaginator::new(lines, PaginatorOptions::new()
        .max_lines(NonZeroUsize::new(10).unwrap())
    ).run(ctx).await?;

    Ok(())
}