use crate::inactivity_decay_inner;
use crate::model::EventNumber;
use crate::model::Game;
use crate::model::LeagueInfo;
use crate::model::Player;
use crate::model::StandingEvent;
use crate::model::StandingEventInner::GameEnd;
use crate::util::base_embed;
use crate::util::checks::is_league_moderator;
use crate::util::rating::advance_approve_pointer;
use crate::BotError;
use crate::Context;
use bson::Bson;
use bson::Bson::Int64;
use bson::Bson::Null;
use bson::Document;
use core::cmp::min;
use core::error::Error;
use core::time::Duration;
use futures::TryStreamExt as _;
use mongodb::bson::doc;
use poise::CreateReply;
use serde::de::DeserializeOwned;
use serenity::all::CreateActionRow;
use serenity::all::CreateButton;
use serenity::all::CreateInteractionResponse;
use serenity::all::ReactionType;

/// attempt to advance the approve pointer (be careful)
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn advance_pointer(
    ctx: Context<'_>,
    #[description = "do not approve this event number and after"] stop_before: Option<EventNumber>,
) -> Result<(), BotError> {
    ctx.defer().await?;

    let LeagueInfo {
        first_unreviewed_event_number,
        ..
    } = ctx
        .data()
        .mongo
        .collection::<LeagueInfo>("league_info")
        .find_one(doc! {})
        .await?
        .expect("league_info struct missing");

    let stopped_before = first_unreviewed_event_number;
    let new_stopped_before = advance_approve_pointer(ctx.data(), stop_before).await?;

    ctx.reply(match stopped_before == new_stopped_before {
        true => format!("ok, stopped at event number {stopped_before} (no change)"),
        false => format!(
            "ok, previously was stopped before event number {stopped_before}, \
        now stopped before event number {new_stopped_before}"
        ),
    })
    .await?;

    Ok(())
}

/// move the advance pointer back to 0, clear all ratings
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn force_reprocess(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.data()
        .mongo
        .collection::<LeagueInfo>("league_info")
        .update_one(
            doc! {},
            doc! { "$set": {"first_unreviewed_event_number": Int64(0) } },
        )
        .await?;

    ctx.data()
        .mongo
        .collection::<Player>("players")
        .update_many(
            doc! {},
            doc! {"$set": {
                "rating": 0,
                "deviation": 0,
                "last_played": Null
            }},
        )
        .await?;

    ctx.reply("ok").await?;
    Ok(())
}

fn try_make<T>(doc: Document) -> Result<T, Box<dyn Error>>
where
    T: DeserializeOwned,
{
    let parsed: T = bson::deserialize_from_document(doc)?;
    Ok(parsed)
}

/// check integrity of event log
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn fsck(
    ctx: Context<'_>,
    #[description = "attempt repairs"] repair: Option<bool>,
) -> Result<(), BotError> {
    // TODO: check players collection and counter, check validity of player references
    ctx.defer().await?;

    let mut had_err = false;

    let mut first_missing_event = 0;
    let mut first_unreviewed_event = 0;
    let mut first_missing_game = 0;

    let mut events = ctx
        .data()
        .mongo
        .collection::<Document>("events")
        .find(doc! {})
        .sort(doc! {"_id": 1})
        .await?;
    while let Some(out) = events.try_next().await? {
        let to_send = match try_make::<StandingEvent>(out.clone()) {
            Ok(evt) => {
                if evt.id != first_missing_event {
                    let out = format!("event {first_missing_event} is missing");
                    first_missing_event = evt.id + 1;
                    out
                } else {
                    first_missing_event += 1;
                    match evt.approval_status {
                        None => first_unreviewed_event = evt.id,
                        Some(_) => {
                            first_unreviewed_event = if first_unreviewed_event == evt.id {
                                evt.id + 1
                            } else {
                                first_unreviewed_event
                            }
                        }
                    }

                    if let GameEnd(Game { game_id, .. }) = evt.inner {
                        if game_id != first_missing_game {
                            let out = format!("game {first_missing_game} is missing");
                            first_missing_game = game_id + 1;
                            out
                        } else {
                            first_missing_game += 1;
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            }
            Err(e) => {
                let offender: &Bson = out
                    .get("_id")
                    .expect("how does a mongo object not have an id");
                format!("event {offender} is not okay:\n{e:?}")
            }
        };
        // we will never be here if everything is okay
        had_err = true;
        ctx.reply(to_send).await?;
    }

    let Some(league_info) = ctx
        .data()
        .mongo
        .collection::<LeagueInfo>("league_info")
        .find_one(doc! {})
        .await?
    else {
        ctx.reply("league_info DNE").await?;
        return Ok(());
    };

    if league_info.available_event_number != first_missing_event {
        ctx.reply(format!("league_info available event number {} != actual {first_missing_event}, INSPECT AND FIX",
                          league_info.available_event_number)).await?;
        had_err = true;
    }

    if league_info.first_unreviewed_event_number != first_unreviewed_event {
        ctx.reply(format!(
            "league_info unreviewed event number {} != actual {first_unreviewed_event}",
            league_info.first_unreviewed_event_number
        ))
        .await?;
        had_err = true;
    }

    if league_info.available_game_id != first_missing_game {
        ctx.reply(format!(
            "league_info available game number {} != actual {first_missing_game}, INSPECT AND FIX",
            league_info.available_game_id
        ))
        .await?;
        had_err = true;
    }

    if !had_err {
        ctx.reply("all ok").await?;
    } else if repair.unwrap_or(false) {
        let mut fix_league_info = league_info;
        fix_league_info.available_event_number =
            min(fix_league_info.available_event_number, first_missing_event);
        fix_league_info.available_game_id =
            min(fix_league_info.available_game_id, first_missing_game);
        fix_league_info.first_unreviewed_event_number = first_unreviewed_event;
        ctx.data()
            .mongo
            .collection::<LeagueInfo>("league_info")
            .find_one_and_replace(doc! {}, fix_league_info)
            .await?;
        ctx.reply("fixing approve pointer, trimming free event/game numbers as necessary")
            .await?;
    }

    Ok(())
}

/// forcibly do deviation decay now
#[poise::command(prefix_command, slash_command, owners_only)]
pub(crate) async fn do_decay(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.defer().await?;

    inactivity_decay_inner(&ctx.data().mongo).await?;
    ctx.reply("ok").await?;

    Ok(())
}

/// league moderators: remove the latest event from the record irreversibly
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn pop_event(ctx: Context<'_>) -> Result<(), BotError> {
    let mutex = ctx.data().core_state_lock.clone();
    mutex.lock().await;

    let LeagueInfo {
        available_event_number,
        ..
    } = ctx
        .data()
        .mongo
        .collection::<LeagueInfo>("league_info")
        .find_one(doc! {})
        .await?
        .expect("league_info struct missing");

    let Some(victim_event) = ctx
        .data()
        .mongo
        .collection::<StandingEvent>("events")
        .find_one(doc! { "_id": available_event_number - 1 })
        .await?
    else {
        ctx.reply("latest event DNE; you have a major issue, fsck now")
            .await?;
        return Ok(());
    };

    let handle = ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .description(format!(
                "**you are permanently removing event ID {}:**\n> {}\n**from the record!** please confirm (5 seconds)",
                victim_event.id, victim_event.short_summary(&ctx.data().mongo).await?)))
        .components(vec![
            CreateActionRow::Buttons(vec![
                CreateButton::new("pop_event_confirm")
                    .emoji(ReactionType::Unicode(String::from("✅")))
            ])
        ])
        .reply(true)
    ).await?;

    match handle
        .message()
        .await?
        .await_component_interaction(&ctx.serenity_context().shard)
        .author_id(ctx.author().id)
        .custom_ids(vec![String::from("pop_event_confirm")])
        .timeout(Duration::from_secs(10))
        .await
    {
        None => {
            ctx.reply("ok, nevermind then").await?;
            return Ok(());
        }
        Some(ixn) => {
            ixn.create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await?;
        }
    }

    // yes, this is declared twice but no big deal tbh
    let evt = match ctx
        .data()
        .mongo
        .collection::<StandingEvent>("events")
        .find_one_and_delete(doc! { "_id": victim_event.id })
        .await?
    {
        None => {
            ctx.reply("free event number bad?").await?;
            return Ok(());
        }
        Some(evt) => {
            let update_doc = if let GameEnd(_) = evt.inner {
                doc! {
                    "$inc": {"available_event_number": -1, "available_game_id": -1},
                    "$min": { "first_unreviewed_event_number": evt.id }
                }
            } else {
                doc! {
                    "$inc": { "available_event_number": -1 },
                    "$min": { "first_unreviewed_event_number": evt.id }
                }
            };

            ctx.data()
                .mongo
                .collection::<LeagueInfo>("league_info")
                .update_one(doc! {}, update_doc)
                .await?;

            evt
        }
    };

    ctx.reply(format!(
        "ok, event {} is gone, need to reprocess to finish",
        evt.id
    ))
    .await?;
    Ok(())
}
