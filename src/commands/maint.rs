use crate::model::{EventNumber, LeagueInfo, Player, StandingEvent};
use crate::util::checks::is_league_moderator;
use crate::util::rating::advance_approve_pointer;
use crate::{BotError, Context};
use bson::Bson::Int64;
use bson::{doc, Bson, Document};
use futures::TryStreamExt;
use itertools::Itertools;
use serde::de::DeserializeOwned;
use std::error::Error;

/// attempt to advance the approve pointer (be careful)
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn advance_pointer(
    ctx: Context<'_>,
    #[description = "do not approve this event number and after"] stop_before: Option<EventNumber>,
) -> Result<(), BotError> {
    let LeagueInfo { first_unreviewed_event_number, .. } = ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one(doc! {})
        .await?
        .expect("league_info struct missing");

    let stopped_before = first_unreviewed_event_number;
    let new_stopped_before = advance_approve_pointer(&ctx.data(), stop_before).await?;

    ctx.reply(match stopped_before == new_stopped_before {
        true => format!("ok, stopped at event number {} (no change)", stopped_before),
        false => format!("ok, previously was stopped before event number {stopped_before}, \
        now stopped before event number {new_stopped_before}")
    }).await?;

    Ok(())
}

/// move the advance pointer back to 0, clear all ratings
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn force_reprocess(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .update_one(doc! {}, doc! { "$set": {"first_unreviewed_event_number": Int64(0) } })
        .await?;

    ctx.data().mongo.collection::<Player>("players").update_many(doc! {}, doc! { "$set": { "rating": 0, "deviation": 0 } }).await?;

    ctx.reply("ok").await?;
    Ok(())
}

fn try_make<T>(doc: Document) -> Result<T, Box<dyn Error>>
where
    T: DeserializeOwned,
{
    let parsed: T = bson::from_document(doc)?;
    Ok(parsed)
}

/// check integrity of event log
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn fsck(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.defer().await?;

    let mut had_err = false;

    let mut events = ctx.data().mongo.collection::<Document>("events").find(doc! {}).await?;
    while let Some(out) = events.try_next().await? {
        let to_send = match try_make::<StandingEvent>(out.clone()) {
            Ok(_) => continue,
            Err(e) => {
                let offender: &Bson = out.get("_id").expect("how does a mongo object not have an id");
                format!("event {} is not okay:\n{:?}", offender.to_string(), e)
            }
        };
        // we will never be here if everything is okay
        had_err = true;
        ctx.reply(to_send).await?;
    }

    if !had_err {
        ctx.reply("all events look ok").await?;
    }
    Ok(())
}

/// check integrity of event log
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn migrate(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.defer().await?;

    let all_players = ctx.data().mongo.collection::<Player>("players2").find(doc! {})
        .sort(doc! { "_id": 1 })
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    ctx.data().mongo.collection::<Player>("players").insert_many(all_players).await?;

    let all_events = ctx.data().mongo.collection::<StandingEvent>("events2").find(doc! {})
        .sort(doc! { "_id": 1 })
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    ctx.data().mongo.collection::<StandingEvent>("events").insert_many(all_events).await?;

    ctx.reply("ok").await?;
    Ok(())
}