use crate::model::{Game, GameID, LeagueInfo};
use crate::BotError;
use bson::doc;

pub(crate) async fn advance_approve_pointer(mongo: &mongodb::Client, mongo_db_name: &str, pg_conn: &deadpool_postgres::Object) -> Result<GameID, BotError> {
    let mut sess = mongo.start_session().await?;
    sess.start_transaction().await?;

    let league_info_collection = mongo.database(mongo_db_name).collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");

    let mut first_unreviewed_game = league_info.first_unreviewed_game;
    loop {
        match mongo.database(&*mongo_db_name).collection::<Game>("games").find_one(doc! { "_id": first_unreviewed_game as i64 })
            .await? {
            Some(game) => match game.approval_status {
                Some(approval_status) =>
                    {
                        first_unreviewed_game += 1;
                        if approval_status.approved {
                            // TODO
                            // let trans = pg_conn.build_transaction()
                            //     .isolation_level();
                            // calculate new ratings
                        }
                    }
                None => break,
            }
            None => break,
        };
    }

    league_info_collection.find_one_and_update(doc! {}, doc! {
        "first_unreviewed_game": first_unreviewed_game as i64,
    }).await?;

    Ok(first_unreviewed_game)
}