use crate::model::StandingEventInner::{ChangeStanding, GameEnd, InactivityDecay, JoinLeague, Penalty};
use crate::model::{EventNumber, LeagueInfo, StandingEvent, StandingEventInner};
use crate::util::constants::TRUESKILL_CONFIG;
use crate::{BotError, BotVars};
use bson::doc;
use futures::StreamExt;
use itertools::Itertools;
use skillratings::trueskill::{expected_score_multi_team, trueskill_multi_team, TrueSkillRating};
use skillratings::MultiTeamOutcome;
use tokio_postgres::types::Type;

pub(crate) trait RatingExtra {
    fn from_row(row: &tokio_postgres::Row) -> Self;
    fn is_provisional(&self) -> bool;
    fn leaderboard_rating(&self) -> f64;
    fn format_rating(&self) -> String;
}

impl RatingExtra for TrueSkillRating {
    fn from_row(row: &tokio_postgres::Row) -> Self {
        Self::from((row.get("rating"), row.get("deviation")))
    }

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

/// check for any unreviewed events (right now, these are only games) and update the record of present-day ratings in SQL.
/// the "approve pointer" in the function name, or the first unreviewed event, is advanced until it actually points to an unreviewed event
/// along the way, we process the results of any standing events we find
pub(crate) async fn advance_approve_pointer(data: &BotVars, stop_before: Option<EventNumber>) -> Result<EventNumber, BotError> {
    let mutex = data.update_ratings_lock.clone();
    mutex.lock().await;

    let mut pg_conn = data.postgres.get().await?;
    let pg_trans = pg_conn.build_transaction().start().await?;

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
                    inner.process_effect(&pg_trans).await?;
                }
            }
        }
    }

    pg_trans.commit().await?;
    league_info_collection.find_one_and_update(doc! {}, doc! {
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

    pub(crate) async fn process_effect(&self, pg_trans: &deadpool_postgres::Transaction<'_>) -> Result<(), BotError> {
        let self_processable = match self {
            Penalty { .. } | InactivityDecay { .. } => &self.clone()
                .try_into_generic_variant().expect("1984"),
            _ => self
        };

        match self_processable {
            GameEnd(game) => {
                let prepared_select = pg_trans.prepare_typed_cached(
                    "SELECT rating, deviation FROM players WHERE player_id = $1;",
                    &[Type::INT4]).await?;
                let prepared_update = pg_trans.prepare_typed_cached(
                    "UPDATE players SET rating = $1, deviation = $2 WHERE player_id = $3;",
                    &[Type::FLOAT8, Type::FLOAT8, Type::INT4]).await?;

                let mut old_ratings = Vec::with_capacity(game.ranking.len());
                for party_id in game.ranking.iter() {
                    let row = pg_trans.query_one(&prepared_select, &[party_id]).await?;
                    old_ratings.push(TrueSkillRating::from_row(&row));
                }

                let new_ratings = game_affect_ratings(&old_ratings);
                for (party_id, new_rating) in game.ranking.iter().zip(new_ratings.into_iter()) {
                    pg_trans.execute(&prepared_update, &[&new_rating.rating, &new_rating.uncertainty, party_id]).await?;
                }
            }
            ChangeStanding { victims, delta_rating, delta_deviation, .. } => {
                if let Some(delta_rating) = delta_rating {
                    pg_trans.execute("UPDATE players SET rating = rating + $1 WHERE player_id = ANY($2);",
                                     &[delta_rating, &victims]).await?;
                }

                if let Some(delta_deviation) = delta_deviation {
                    pg_trans.execute("UPDATE players SET deviation = deviation + $1 WHERE player_id = ANY($2);",
                                     &[delta_deviation, &victims]).await?;
                }
            }
            JoinLeague { victims, initial_rating, initial_deviation } => {
                pg_trans.execute("UPDATE players SET rating = $1, deviation = $2 WHERE player_id = ANY($3);",
                                 &[initial_rating, initial_deviation, victims]).await?;
            }
            _ => return Err("don't know how to handle this event type yet".into())
        }

        Ok(())
    }
}
