use mongodb::bson::serde_helpers::serialize_bson_datetime_as_rfc3339_string;
use serde::{Deserialize, Serialize};
use skillratings::trueskill::TrueSkillRating;
use std::collections::{HashMap, HashSet};
use mongodb::bson;

pub(crate) type PlayerID = i32;
type GameID = u64;

struct LeagueInfo {
    last_approved: GameID,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Game {
    pub(crate) _id: GameID,
    // in placement order
    pub(crate) participants: Vec<PlayerID>,
    // seconds long
    pub(crate) length: u32,
    // time submitted to system
    #[serde(serialize_with = "serialize_bson_datetime_as_rfc3339_string")]
    pub(crate) when: bson::DateTime,
    pub(crate) approver: Option<PlayerID>,
}

enum StandingEventVariant {
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
    event_type: StandingEventVariant,
    when: bson::DateTime,
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}