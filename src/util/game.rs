use crate::model::StandingEventVariant::GameEnd;
use crate::model::{EventNumber, Game, LeagueInfo, StandingEvent};
use crate::util::rating::{game_affect_ratings, RatingExtra};
use crate::{BotError, BotVars};
use bson::doc;
use futures::StreamExt;
use skillratings::trueskill::TrueSkillRating;
use tokio_postgres::types::Type;

/// check for any unreviewed events (right now, these are only games) and update the record of present-day ratings in SQL.
/// the "approve pointer" in the function name, or the first unreviewed event, is advanced until it actually points to an unreviewed event
/// along the way, we process the results of any standing events we find
pub(crate) async fn advance_approve_pointer(data: &BotVars) -> Result<EventNumber, BotError> {
    let mutex = data.update_ratings_lock.clone();
    mutex.lock().await;

    let mut pg_conn = data.postgres.get().await?;
    let pg_trans = pg_conn.build_transaction().start().await?;

    let league_info_collection = data.mongo.collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");
    let mut first_unreviewed_event_number_num = league_info.first_unreviewed_event_number;

    let mut allegedly_unreviewed = data.mongo.collection::<StandingEvent>("events")
        .find(doc! { "_id": doc! {"$gt": first_unreviewed_event_number_num } })
        .sort(doc! { "_id": 1 }).await?;

    let prepared_select = pg_trans.prepare_typed_cached("SELECT rating, deviation FROM players WHERE player_id = $1;",
                                                        &[Type::INT4]).await?;
    let prepared_update = pg_trans.prepare_typed_cached("UPDATE players SET rating = $1, deviation = $2 WHERE player_id = $3;",
                                                        &[Type::FLOAT8, Type::FLOAT8, Type::INT4]).await?;

    while let Some(standing_event) = allegedly_unreviewed.next().await {
        let standing_event = standing_event?;
        if let StandingEvent { event_type: GameEnd { game_id }, approval_status, .. } = standing_event {
            // TODO: things other than games exist here
            let game = data.mongo.collection::<Game>("games").find_one(doc! { "_id": game_id }).await?
                .expect("standing event points to game which DNE");

            match approval_status {
                None => break,
                Some(approval_status) => {
                    first_unreviewed_event_number_num += 1;
                    if approval_status.approved {
                        let mut old_ratings = Vec::with_capacity(game.participants.len());
                        for party_id in game.participants.iter() {
                            let row = pg_trans.query_one(&prepared_select, &[party_id]).await?;
                            old_ratings.push(TrueSkillRating::from_row(row));
                        }

                        let new_ratings = game_affect_ratings(&old_ratings);
                        for (party_id, new_rating) in game.participants.into_iter().zip(new_ratings.into_iter()) {
                            pg_trans.execute(&prepared_update, &[&new_rating.rating, &new_rating.uncertainty, &party_id]).await?;
                        }
                    }
                }
            }
        }
    }

    pg_trans.commit().await?;
    league_info_collection.find_one_and_update(doc! {}, doc! {
        "$max": doc! { "first_unreviewed_event_number": first_unreviewed_event_number_num as i64 },
    }).await?;

    Ok(first_unreviewed_event_number_num)
}