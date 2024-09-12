use crate::model::{EventNumber, StandingEvent};
use crate::util::paginate::EmbedLinePaginator;
use crate::{BotError, Context};
use bson::doc;
use futures::TryStreamExt;

#[poise::command(prefix_command, slash_command, subcommands("log"))]
pub(crate) async fn event(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.reply("base command is noop, try a subcommand").await?;

    Ok(())
}

/// Get a reverse-chronological ordered log of every event
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn log(
    ctx: Context<'_>,
    #[description = "skip events after this event number"] before: Option<EventNumber>,
) -> Result<(), BotError> {
    ctx.defer().await?;

    let pg_conn = ctx.data().postgres.get().await?;

    let filter_doc = if before.is_some() {
        doc! { "_id": doc! { "$lte": before.unwrap() } }
    } else {
        doc! {}
    };

    let mut lines = Vec::new();
    let mut cur = ctx.data().mongo.collection::<StandingEvent>("events")
        .find(filter_doc)
        .sort(doc! { "_id": -1 })
        .limit(200)
        .await?;
    while let Some(event) = cur.try_next().await? { lines.push(event.short_summary(&pg_conn).await?) }

    EmbedLinePaginator::new(lines)
        .run(ctx).await?;

    Ok(())
}