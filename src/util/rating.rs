use crate::commands::ewar::user::try_lookup_player;
use crate::commands::ewar::user::UserLookupType::SystemID;
use crate::model::StandingEventInner::{ChangeStanding, GameEnd, InactivityDecay, JoinLeague, Penalty};
use crate::model::{EventNumber, LeagueInfo, Player, StandingEvent, StandingEventInner};
use crate::util::constants::TRUESKILL_CONFIG;
use crate::{BotError, BotVars};
use bson::doc;
use futures::StreamExt;
use itertools::Itertools;
use mongodb::Database;
use skillratings::trueskill::{expected_score_multi_team, trueskill_multi_team, TrueSkillRating};
use skillratings::MultiTeamOutcome;

pub(crate) trait RatingExtra {
    fn is_provisional(&self) -> bool;
    fn leaderboard_rating(&self) -> f64;
    fn format_rating(&self) -> String;
}

impl RatingExtra for TrueSkillRating {
    fn is_provisional(&self) -> bool {
        self.uncertainty - 2.5 > f64::EPSILON
    }

    fn leaderboard_rating(&self) -> f64 {
        if self.is_provisional() {
            10f64 * (self.rating - self.uncertainty)
        } else {
            10f64 * self.rating
        }
    }

    fn format_rating(&self) -> String {
        format!("{:.2}{}", self.leaderboard_rating(), if self.is_provisional() { "**?**" } else { "" })
    }
}

pub(crate) fn expected_outcome(placement: &Vec<TrueSkillRating>) -> Vec<f64> {
    let ratings = placement.iter()
        .map(|rating| vec![*rating])
        .collect_vec();

    expected_score_multi_team(
        ratings.iter()
            .map(|rating| &rating[..])
            .collect_vec()
            .as_slice(),
        &TRUESKILL_CONFIG)
}

pub(crate) fn game_affect_ratings(placement: &Vec<TrueSkillRating>) -> Vec<TrueSkillRating> {
    let ratings = placement.iter()
        .map(|rating| vec![*rating])
        .collect_vec();

    // each team has exactly 1 player
    trueskill_multi_team(
        ratings.iter()
            .enumerate()
            .map(|(index, rating)| (&rating[..], MultiTeamOutcome::new(index + 1)))
            .collect_vec()
            .as_slice(),
        &TRUESKILL_CONFIG).into_iter()
        .map(|team| team[0])
        .collect_vec()
}

/// check for any unreviewed events (right now, these are only games) and update the record of present-day ratings.
/// the "approve pointer" in the function name, or the first unreviewed event, is advanced until it actually points to an unreviewed event
/// along the way, we process the results of any standing events we find
pub(crate) async fn advance_approve_pointer(data: &BotVars, stop_before: Option<EventNumber>) -> Result<EventNumber, BotError> {
    let mutex = data.update_ratings_lock.clone();
    mutex.lock().await;

    let league_info_collection = data.mongo.collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");
    let mut first_unreviewed_event_number_num = league_info.first_unreviewed_event_number;

    let mut allegedly_unreviewed = data.mongo.collection::<StandingEvent>("events")
        .find(doc! { "_id": doc! {"$gte": first_unreviewed_event_number_num } })
        .sort(doc! { "_id": 1 }).await?;

    while let Some(standing_event) = allegedly_unreviewed.next().await {
        if first_unreviewed_event_number_num >= stop_before.unwrap_or(EventNumber::MAX) { break; }

        let standing_event = standing_event?;
        let StandingEvent { inner, approval_status, .. } = standing_event;
        match approval_status {
            None => break,
            Some(approval_status) => {
                first_unreviewed_event_number_num += 1;
                if approval_status.approved {
                    inner.process_effect(&data.mongo).await?;
                }
            }
        }
    }

    league_info_collection.update_one(doc! {}, doc! {
        "$max": doc! { "first_unreviewed_event_number": first_unreviewed_event_number_num as i64 },
    }).await?;

    Ok(first_unreviewed_event_number_num)
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

    pub(crate) async fn process_effect(&self, mongo: &Database) -> Result<(), BotError> {
        let self_processable = match self {
            Penalty { .. } | InactivityDecay { .. } => &self.clone()
                .try_into_generic_variant().expect("1984"),
            _ => self
        };

        match self_processable {
            GameEnd(game) => {
                let mut old_ratings = Vec::with_capacity(game.ranking.len());
                for party_id in game.ranking.iter() {
                    let player = try_lookup_player(mongo, SystemID(*party_id)).await?.expect("party to game DNE");
                    old_ratings.push(player.rating_struct());
                }

                let new_ratings = game_affect_ratings(&old_ratings);
                for (party_id, new_rating) in game.ranking.iter().zip(new_ratings.into_iter()) {
                    mongo.collection::<Player>("players").update_one(
                        doc! { "_id" : *party_id },
                        doc! { "rating": new_rating.rating, "deviation": new_rating.uncertainty },
                    ).await?;
                }
            }
            ChangeStanding { victims, delta_rating, delta_deviation, .. } => {
                if let Some(delta_rating) = delta_rating {
                    mongo.collection::<Player>("players").update_many(
                        doc! { "_id": doc! { "$in": victims } },
                        doc! { "$inc": doc! { "rating": delta_rating } },
                    ).await?;
                }

                if let Some(delta_deviation) = delta_deviation {
                    mongo.collection::<Player>("players").update_many(
                        doc! { "_id": doc! { "$in": victims } },
                        doc! { "$inc": doc! { "deviation": delta_deviation } },
                    ).await?;
                }
            }
            JoinLeague { victims, initial_rating, initial_deviation } => {
                mongo.collection::<Player>("players").update_many(
                    doc! { "_id": doc! { "$in": victims } },
                    doc! { "set": doc! { "rating": initial_rating, "deviation": initial_deviation } },
                ).await?;
            }
            _ => return Err("don't know how to handle this event type yet".into())
        }

        Ok(())
    }
}
