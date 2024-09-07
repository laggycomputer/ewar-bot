use chrono::Utc;
use serde::{Deserialize, Serialize};
use skillratings::trueskill::TrueSkillRating;
use std::collections::HashMap;

pub(crate) type PlayerID = i32;
pub(crate) type GameID = u64;

#[derive(Serialize, Deserialize)]
pub(crate) struct LeagueInfo {
    last_not_approved_game: GameID,
    pub(crate) available_game_id: GameID,
    pub(crate) available_event_number: EventNumber,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Game {
    pub(crate) _id: GameID,
    // in placement order
    pub(crate) participants: Vec<PlayerID>,
    // seconds long
    pub(crate) length: u32,
    // time submitted to system
    pub(crate) when: chrono::DateTime<Utc>,
    pub(crate) approver: Option<PlayerID>,
    pub(crate) event_number: EventNumber,
}

#[derive(Serialize, Deserialize)]
pub(crate) enum StandingEventVariant {
    // remove rating for foul play
    Penalty { amount: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { amount: f64 },
    // regular game
    GameEnd { game_id: GameID },
}

type EventNumber = u32;

#[derive(Serialize, Deserialize)]
pub(crate) struct StandingEvent {
    pub(crate) _id: EventNumber,
    pub(crate) affected: Vec<PlayerID>,
    pub(crate) event_type: StandingEventVariant,
    pub(crate) when: chrono::DateTime<Utc>,
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}