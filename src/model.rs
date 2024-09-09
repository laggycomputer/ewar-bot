use chrono::Utc;
use serde::{Deserialize, Serialize};
use skillratings::trueskill::TrueSkillRating;
use std::collections::HashMap;

pub(crate) type EventNumber = u32;
pub(crate) type GameID = i64;
pub(crate) type PlayerID = i32;

#[derive(Serialize, Deserialize)]
pub(crate) struct LeagueInfo {
    pub(crate) first_unreviewed_event_number: EventNumber,
    pub(crate) available_game_id: GameID,
    pub(crate) available_event_number: EventNumber,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ApprovalStatus {
    pub(crate) approved: bool,
    pub(crate) reviewer: PlayerID,
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
    pub(crate) event_number: EventNumber,
}

#[derive(Serialize, Deserialize)]
pub(crate) enum StandingEventInner {
    // remove rating for foul play
    Penalty { victim: Vec<PlayerID>, amount: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { victim: Vec<PlayerID>, amount: f64 },
    // regular game
    GameEnd { game_id: GameID },
}

#[derive(Serialize, Deserialize)]
pub(crate) struct StandingEvent {
    pub(crate) _id: EventNumber,
    pub(crate) approval_status: Option<ApprovalStatus>,
    pub(crate) inner: StandingEventInner,
    pub(crate) when: chrono::DateTime<Utc>,
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}