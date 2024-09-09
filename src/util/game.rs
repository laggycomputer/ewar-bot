use crate::model::{Game, GameID, LeagueInfo};
use crate::util::rating::{game_affect_ratings, RatingExtra};
use crate::{BotError, BotVars};
use bson::doc;
use futures::StreamExt;
use skillratings::trueskill::TrueSkillRating;
use tokio_postgres::types::Type;

pub(crate) async fn advance_approve_pointer(data: &BotVars) -> Result<GameID, BotError> {
    let mutex = data.update_ratings_lock.clone();
    mutex.lock().await;

    let mut pg_conn = data.postgres.get().await?;
    let pg_trans = pg_conn.build_transaction().start().await?;

    let league_info_collection = data.mongo.collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");
    let mut first_unreviewed_game = league_info.first_unreviewed_game;

    let mut allegedly_unreviewed = data.mongo.collection::<Game>("games")
        .find(doc! { "_id": doc! {"$gt": first_unreviewed_game as i64 } })
        .sort(doc! { "_id": 1 }).await?;

    let prepared_select = pg_trans.prepare_typed_cached("SELECT rating, deviation FROM players WHERE player_id = $1;",
                                                       &[Type::INT4]).await?;
    let prepared_update = pg_trans.prepare_typed_cached("UPDATE players SET rating = $1, deviation = $2 WHERE player_id = $3;",
                                                       &[Type::FLOAT8, Type::FLOAT8, Type::INT4]).await?;

    while let Some(game) = allegedly_unreviewed.next().await {
        let game = game?;
        match game.approval_status {
            None => break,
            Some(approval_status) => {
                first_unreviewed_game += 1;
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

    pg_trans.commit().await?;
    league_info_collection.find_one_and_update(doc! {}, doc! {
        "$max": doc! { "first_unreviewed_game": first_unreviewed_game as i64 },
    }).await?;

    Ok(first_unreviewed_game)
}