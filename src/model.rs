use mongodb::bson::serde_helpers::serialize_bson_datetime_as_rfc3339_string;
use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};

type PlayerID = (i64, i32);

#[derive(Serialize, Deserialize)]
struct Game {
    id: u64,
    // in placement order
    participants: Vec<PlayerID>,
    // seconds long
    length: u32,
    // time submitted to system
    #[serde(serialize_with = "serialize_bson_datetime_as_rfc3339_string")]
    time: DateTime,
}

enum StandingEvent {
    // remove rating for foul play
    Penalty { affected: PlayerID, amount: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { affected: PlayerID, amount: f64 },
    // regular game
    GameEnd { game: u64 },
}