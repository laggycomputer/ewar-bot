use mongodb::bson::serde_helpers::serialize_bson_datetime_as_rfc3339_string;
use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};
use serenity::all::UserId;
use skillratings::trueskill::TrueSkillRating;
use std::collections::{HashMap, HashSet};

type PlayerID = UserId;
type GameID = u64;

#[derive(Serialize, Deserialize)]
struct Game {
    id: u64,
    // in placement order
    participants: Vec<PlayerID>,
    // seconds long
    length: u32,
    // time submitted to system
    #[serde(serialize_with = "serialize_bson_datetime_as_rfc3339_string")]
    when: DateTime,
}

enum StandingEventType {
    // remove rating for foul play
    Penalty { amount: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { amount: f64 },
    // regular game
    GameEnd { game: Game },
}

type EventNumber = u32;

struct StandingEvent {
    number: EventNumber,
    affected: HashSet<PlayerID>,
    event_type: StandingEventType,
    when: DateTime,
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}