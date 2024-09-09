use crate::model::StandingEventInner::{ChangeStanding, GameEnd, InactivityDecay, Penalty};
use crate::util::rating::{game_affect_ratings, RatingExtra};
use crate::{BotError, BotVars};
use bson::doc;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use skillratings::trueskill::TrueSkillRating;
use std::collections::HashMap;
use tokio_postgres::types::Type;

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

#[derive(Serialize, Deserialize, Clone)]
#[non_exhaustive]
pub(crate) enum StandingEventInner {
    // remove rating for foul play
    Penalty { victims: Vec<PlayerID>, delta_rating: f64, reason: String },
    // add deviation for inactivity
    InactivityDecay { victims: Vec<PlayerID>, delta_deviation: f64 },
    // regular game
    GameEnd { game_id: GameID },
    SetStanding { victims: Vec<PlayerID>, new_rating: Option<f64>, new_deviation: Option<f64>, reason: String },
    ChangeStanding { victims: Vec<PlayerID>, delta_rating: Option<f64>, delta_deviation: Option<f64>, reason: String },
}

impl StandingEventInner {
    /// convert to a different type to simplify handling
    fn try_into_generic_variant(self) -> Option<Self> {
        match self {
            Penalty { victims, delta_rating, reason } => Some(ChangeStanding {
                victims,
                delta_rating: Some(delta_rating),
                reason,
                delta_deviation: None,
            }),
            InactivityDecay { victims, delta_deviation } => Some(ChangeStanding {
                victims,
                delta_rating: None,
                delta_deviation: Some(delta_deviation),
                reason: String::new(),
            }),
            _ => None
        }
    }

    pub(crate) async fn process_effect(&self, data: &BotVars, pg_trans: &deadpool_postgres::Transaction<'_>) -> Result<(), BotError> {
        let self_processable = match self {
            Penalty { .. } | InactivityDecay { .. } => &self.clone()
                .try_into_generic_variant().expect("1984"),
            _ => self
        };

        match self_processable {
            GameEnd { game_id } => {
                let prepared_select = pg_trans.prepare_typed_cached(
                    "SELECT rating, deviation FROM players WHERE player_id = $1;",
                    &[Type::INT4]).await?;
                let prepared_update = pg_trans.prepare_typed_cached(
                    "UPDATE players SET rating = $1, deviation = $2 WHERE player_id = $3;",
                    &[Type::FLOAT8, Type::FLOAT8, Type::INT4]).await?;

                let game = data.mongo.collection::<Game>("games").find_one(doc! { "_id": game_id }).await?
                    .expect("standing event points to game which DNE");

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
            ChangeStanding { victims, delta_rating, delta_deviation, .. } => {
                if let Some(delta_rating) = delta_rating {
                    pg_trans.execute("UPDATE players SET rating = rating + $1 WHERE player_id = ANY($2);", &[delta_rating, &victims]).await?;
                }

                if let Some(delta_deviation) = delta_deviation {
                    pg_trans.execute("UPDATE players SET deviation = deviation + $1 WHERE player_id = ANY($2);", &[delta_deviation, &victims]).await?;
                }
            }
            _ => return Err("don't know how to handle this event type yet".into())
        }

        Ok(())
    }
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