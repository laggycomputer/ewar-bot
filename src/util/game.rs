use crate::model::{Game, GameID, LeagueInfo};
use crate::{BotError, BotVars};
use bson::doc;

pub(crate) async fn advance_approve_pointer(data: &BotVars) -> Result<GameID, BotError> {
    let mut pg_conn = data.postgres.get().await?;
    let pg_trans = pg_conn.build_transaction();

    let league_info_collection = data.mongo.collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");

    let mut first_unreviewed_game = league_info.first_unreviewed_game;
    loop {
            match data.mongo.collection::<Game>("games").find_one(doc! { "_id": first_unreviewed_game as i64 })
            .await? {
            Some(game) => match game.approval_status {
                Some(approval_status) =>
                    {
                        first_unreviewed_game += 1;
                        if approval_status.approved {
                            // TODO
                            // calculate new ratings
                        }
                    }
                None => break,
            }
            None => break,
        };
    }

    league_info_collection.find_one_and_update(doc! {}, doc! {
        "$max": doc! { "first_unreviewed_game": first_unreviewed_game as i64 },
    }).await?;
    Ok(first_unreviewed_game)
}