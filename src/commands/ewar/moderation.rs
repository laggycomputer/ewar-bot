use crate::model::StandingEventInner::GameEnd;
use crate::model::{Game, GameID, PlayerID, StandingEvent};
use crate::util::checks::{has_system_account, is_league_moderator};
use crate::util::rating::advance_approve_pointer;
use crate::{BotError, Context};
use bson::doc;
use poise::CreateReply;

/// League moderators: review game for league record; approve or reject
#[poise::command(prefix_command, slash_command, check = has_system_account, check = is_league_moderator)]
pub(crate) async fn review(
    ctx: Context<'_>,
    #[description = "ID of game to approve"] game_id: GameID,
    #[description = "whether to accept or reject this game"] approved: bool) -> Result<(), BotError> {
    let corresponding_event = match ctx.data().mongo.collection::<StandingEvent>("events").find_one(
        doc! { "inner": doc! { "GameEnd": doc! { "game_id": game_id } } }).await? {
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
