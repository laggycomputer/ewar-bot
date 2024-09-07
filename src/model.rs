use chrono::Utc;
use serde::{Deserialize, Serialize};
use skillratings::trueskill::TrueSkillRating;
use std::collections::{HashMap, HashSet};

pub(crate) type PlayerID = i32;
pub(crate) type GameID = u64;

#[derive(Serialize, Deserialize)]
pub(crate) struct LeagueInfo {
    last_not_approved: GameID,
    pub(crate) last_not_submitted: GameID,
    pub(crate) last_free_event_number: EventNumber,
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
}

enum StandingEventVariant {
    // remove rating for foul play
    Penalty { amount: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { amount: f64 },
    // regular game
    GameEnd { game: GameID },
}

type EventNumber = u32;

struct StandingEvent {
    number: EventNumber,
    affected: HashSet<PlayerID>,
    event_type: StandingEventVariant,
    when: chrono::DateTime<Utc>,
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}