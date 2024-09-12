use crate::commands::ewar::user::try_lookup_user;
use crate::commands::ewar::user::UserLookupType::{DiscordID, SystemID};
use crate::model::StandingEventInner::{GameEnd, Penalty};
use crate::model::{ApprovalStatus, Game, GameID, LeagueInfo, PlayerID, StandingEvent};
use crate::util::checks::{has_system_account, is_league_moderator};
use crate::util::rating::advance_approve_pointer;
use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use bson::{doc, Bson};
use chrono::Utc;
use futures::TryStreamExt;
use itertools::Itertools;
use poise::CreateReply;
use serenity::all::{CreateActionRow, CreateButton, CreateEmbedFooter, CreateInteractionResponse, EmojiId, GuildId};
use std::time::Duration;

/// League moderators: review game for league record; approve or reject
#[poise::command(prefix_command, slash_command, check = has_system_account, check = is_league_moderator
)]
pub(crate) async fn review(
    ctx: Context<'_>,
    #[description = "ID of game to approve"] game_id: GameID,
    #[description = "whether to accept or reject this game"] approved: bool) -> Result<(), BotError> {
    let corresponding_event = match ctx.data().mongo.collection::<StandingEvent>("events").find_one(
        doc! { "inner.GameEnd.game_id": game_id }).await? {
        None => {
            ctx.send(CreateReply::default()
                .content(":x: that game DNE")
                .ephemeral(true)).await?;
            return Ok(());
        }
        Some(game) => game
    };

    let StandingEvent {
        inner: GameEnd(Game { ranking, .. }), ..
    } = corresponding_event else {
        return Err(format!("event resembling game with game ID {game_id} is invalid").into())
    };

    if corresponding_event.approval_status.is_some() {
        ctx.send(CreateReply::default()
            .content(":x: that game already reviewed")
            .ephemeral(true)).await?;
        return Ok(());
    }

    // find the reviewer's system ID
    let pg_conn = ctx.data().postgres.get().await?;

    let StandingEvent { _id: event_number, when, .. } = match pg_conn.query_opt(
        "SELECT player_id FROM player_discord WHERE discord_user_id = $1;",
        &[&(ctx.author().id.get() as i64)]).await? {
        None => {
            ctx.send(CreateReply::default()
                .content(":x: do you have an account on the system?")
                .ephemeral(true)).await?;
            return Ok(());
        }
        Some(row) => {
            let reviewer_id: PlayerID = row.get("player_id");

            ctx.data().mongo.collection::<StandingEvent>("events").find_one_and_update(
                doc! { "_id": corresponding_event._id },
                doc! { "$set": doc! { "approval_status": doc! {
                    "approved": approved,
                    "reviewer": Some(reviewer_id),
                } } })
                .await?
                .expect("standing event magically disappeared")
        }
    };

    if approved {
        // set everyone's last played
        pg_conn.execute("UPDATE players SET last_played = $1 WHERE (last_played IS NULL OR last_played < $1) AND player_id = ANY($2)", &[&when.naive_utc(), &ranking]).await?;

        ctx.send(CreateReply::default()
            .content(format!("approved game {game_id} into league record (event number {event_number})"))).await?;
    } else {
        ctx.send(CreateReply::default()
            .content(format!("rejected game {game_id}, event number {event_number}"))).await?;
    }

    advance_approve_pointer(&ctx.data(), None).await?;
    Ok(())
}


/// League moderators: check for unreviewed games
#[poise::command(prefix_command, slash_command, check = is_league_moderator)]
pub(crate) async fn unreviewed(ctx: Context<'_>) -> Result<(), BotError> {
    let pg_conn = ctx.data().postgres.get().await?;

    let find = ctx.data().mongo.collection::<StandingEvent>("events")
        .find(doc! {
            "inner.GameEnd": doc! { "$exists": true },
            "approval_status": Bson::Null,
        })
        .sort(doc! { "_id": 1 })
        .limit(10)
        .await?;

    let events: Vec<_> = find.try_collect().await?;
    if events.is_empty() {
        ctx.reply("no unreviewed games at this time").await?;
        return Ok(());
    }

    let mut event_lines = Vec::with_capacity(events.len());
    for evt in events {
        event_lines.push(format!("#{} - {}", evt._id, evt.short_summary(&pg_conn).await?));
    }

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .description(event_lines.into_iter().join("\n"))
            .footer(CreateEmbedFooter::new("only showing earliest 10 unreviewed games")))
        .reply(true)).await?;

    Ok(())
}

/// League moderators: remove someone's true rating with cause
#[poise::command(prefix_command, slash_command, check = is_league_moderator, check = has_system_account
)]
pub(crate) async fn penalize(
    ctx: Context<'_>,
    #[description = "ID of player to penalize"] target: PlayerID,
    #[description = "amount of true rating to take"] amount: f64,
    #[description = "reason you're doing this"] reason: String,
) -> Result<(), BotError> {
    let pg_conn = ctx.data().postgres.get().await?;
    let victim = match try_lookup_user(&pg_conn, SystemID(target)).await? {
        None => {
            ctx.reply(":x: i don't know who that is").await?;
            return Ok(());
        }
        Some(victim) => victim
    };

    let handle = ctx.send(CreateReply::default()
        .content(format!("**you are penalizing user {} {amount} true rating for {}**\nplease confirm again, you have ten seconds",
                         victim.short_summary(), remove_markdown(&*reason)))
        .components(vec![
            CreateActionRow::Buttons(vec![
                CreateButton::new("penalize_confirm")
                    .emoji(GuildId::new(1278507827442221109u64.try_into().unwrap())
                        .emoji(ctx.http(), EmojiId::new(1283642353395044413u64.try_into().unwrap())).await?)
            ])
        ])
        .reply(true)
    ).await?;

    match handle.message().await?.await_component_interaction(&ctx.serenity_context().shard)
        .author_id(ctx.author().id)
        .custom_ids(vec![String::from("penalize_confirm")])
        .timeout(Duration::from_secs(10)).await {
        None => {
            ctx.reply("ok, nevermind then").await?;
            return Ok(());
        }
        Some(ixn) => ixn.create_response(ctx.http(), CreateInteractionResponse::Acknowledge).await?
    };

    let responsible_moderator = try_lookup_user(&pg_conn, DiscordID(ctx.author().id.get())).await?.unwrap();

    let LeagueInfo { available_event_number, .. } = ctx.data().mongo
        .collection::<LeagueInfo>("league_info")
        .find_one_and_update(
            doc! {},
            doc! { "$inc": doc! { "available_event_number": 1, } })
        .await?
        .expect("league_info struct missing");

    ctx.data().mongo.collection::<StandingEvent>("events").insert_one(StandingEvent {
        _id: available_event_number,
        approval_status: Some(ApprovalStatus {
            approved: true,
            reviewer: Some(responsible_moderator.player_id),
        }),
        inner: Penalty {
            victims: vec![target],
            delta_rating: -amount,
            reason,
        },
        when: Utc::now(),
    }).await?;

    ctx.reply(format!("ok, this is event number {available_event_number} and will take effect as the approve pointer moves forward")).await?;
    advance_approve_pointer(&ctx.data(), None).await?;

    Ok(())
}