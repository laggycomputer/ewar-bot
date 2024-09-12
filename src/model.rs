use bson::doc;
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
    pub(crate) available_player_id: PlayerID,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ApprovalStatus {
    pub(crate) approved: bool,
    // no ID is a system job
    pub(crate) reviewer: Option<PlayerID>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Game {
    pub(crate) game_id: GameID,
    // in placement order
    pub(crate) ranking: Vec<PlayerID>,
    // seconds long
    pub(crate) length: u32,
    // time submitted to system
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[non_exhaustive]
pub(crate) enum StandingEventInner {
    // remove rating for foul play
    Penalty { victims: Vec<PlayerID>, delta_rating: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { victims: Vec<PlayerID>, delta_deviation: f64 },
    // regular game
    GameEnd(Game),
    SetStanding { victims: Vec<PlayerID>, new_rating: Option<f64>, new_deviation: Option<f64>, reason: String },
    ChangeStanding { victims: Vec<PlayerID>, delta_rating: Option<f64>, delta_deviation: Option<f64>, reason: String },
    JoinLeague { victims: Vec<PlayerID>, initial_rating: f64, initial_deviation: f64 },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct StandingEvent {
    pub(crate) _id: EventNumber,
    pub(crate) approval_status: Option<ApprovalStatus>,
    pub(crate) inner: StandingEventInner,
    pub(crate) when: chrono::DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Player {
    pub(crate) _id: PlayerID,
    pub(crate) username: String,
    pub(crate) rating: f64,
    pub(crate) deviation: f64,
    pub(crate) last_played: Option<chrono::DateTime<Utc>>,
    pub(crate) discord_ids: Vec<u64>,
}

impl Player {
    pub(crate) fn rating_struct(&self) -> TrueSkillRating {
        TrueSkillRating { rating: self.rating, uncertainty: self.deviation }
    }
}

// precompute rating at certain points in the timeline
struct Checkpoint {
    after: EventNumber,
    // standings changed since last checkpoint
    updates: HashMap<PlayerID, TrueSkillRating>,
}
