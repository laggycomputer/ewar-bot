use std::cmp::min;
use crate::model::{EventNumber, LeagueInfo, Player, StandingEvent};
use crate::util::checks::is_league_moderator;
use crate::util::rating::advance_approve_pointer;
use crate::{inactivity_decay_inner, BotError, Context};
use bson::Bson::Int64;
use bson::{doc, Bson, Document};
use futures::TryStreamExt;
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
pub(crate) async fn fsck(ctx: Context<'_>, #[description = "attempt repairs"] repair: Option<bool>) -> Result<(), BotError> {
    ctx.defer().await?;

    let mut had_err = false;

    let mut first_missing_event = 0;
    let mut first_unreviewed_event = 0;

    let mut events = ctx.data().mongo.collection::<Document>("events").find(doc! {}).sort(doc! {"_id": 1}).await?;
    while let Some(out) = events.try_next().await? {
        let to_send = match try_make::<StandingEvent>(out.clone()) {
            Ok(evt) => {
                if evt._id != first_missing_event {
                    let out = format!("event {first_missing_event} is missing");
                    first_missing_event = evt._id + 1;
                    out
                } else {
                    first_missing_event += 1;
                    match evt.approval_status {
                        None => first_unreviewed_event = evt._id,
                        Some(_) => first_unreviewed_event = if first_unreviewed_event == evt._id {evt._id + 1} else {first_unreviewed_event},
                    }

                    continue;
                }
            }
            Err(e) => {
                let offender: &Bson = out.get("_id").expect("how does a mongo object not have an id");
                format!("event {} is not okay:\n{:?}", offender.to_string(), e)
            }
        };
        // we will never be here if everything is okay
        had_err = true;
        ctx.reply(to_send).await?;
    }

    let league_info = match ctx.data().mongo.collection::<LeagueInfo>("league_info").find_one(doc! {}).await? {
        None => {
            ctx.reply("league_info DNE").await?;
            return Ok(());
        }
        Some(info) => info
    };

    if league_info.available_event_number != first_missing_event {
        ctx.reply(format!("league_info available event number {} != actual {first_missing_event}", league_info.available_event_number)).await?;
        had_err = true;
    }

    if league_info.first_unreviewed_event_number != first_unreviewed_event {
        ctx.reply(format!("league_info unreviewed event number {} != actual {first_unreviewed_event}", league_info.first_unreviewed_event_number)).await?;
        had_err = true;
    }

    if !had_err {
        ctx.reply("all ok").await?;
    } else {
        if repair.unwrap_or(false) {
            let mut fix_league_info = LeagueInfo::from(league_info);
            fix_league_info.available_event_number = min(fix_league_info.available_event_number, first_missing_event);
            fix_league_info.first_unreviewed_event_number = first_unreviewed_event;
            ctx.data().mongo.collection::<LeagueInfo>("league_info").find_one_and_replace(doc! {}, fix_league_info).await?;
            ctx.reply("fixing approve pointer, trimming free event number as necessary").await?;
        }
    }

    Ok(())
}
