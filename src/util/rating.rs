use crate::model::{EventNumber, LeagueInfo, StandingEvent};
use crate::{BotError, BotVars};
use bson::doc;
use futures::StreamExt;
use itertools::Itertools;
use skillratings::trueskill::{trueskill_multi_team, TrueSkillConfig, TrueSkillRating};
use skillratings::MultiTeamOutcome;

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
        &TrueSkillConfig {
            draw_probability: 0f64,
            beta: 2f64,
            // aka tau
            default_dynamics: 0.04,
        }).into_iter()
        .map(|team| team[0])
        .collect_vec()
}

/// check for any unreviewed events (right now, these are only games) and update the record of present-day ratings in SQL.
/// the "approve pointer" in the function name, or the first unreviewed event, is advanced until it actually points to an unreviewed event
/// along the way, we process the results of any standing events we find
pub(crate) async fn advance_approve_pointer(data: &BotVars) -> Result<EventNumber, BotError> {
    let mutex = data.update_ratings_lock.clone();
    mutex.lock().await;

    let mut pg_conn = data.postgres.get().await?;
    let pg_trans = pg_conn.build_transaction().start().await?;

    let league_info_collection = data.mongo.collection::<LeagueInfo>("league_info");
    let league_info = league_info_collection.find_one(doc! {}).await?
        .expect("league_info struct missing");
    let mut first_unreviewed_event_number_num = league_info.first_unreviewed_event_number;

    let mut allegedly_unreviewed = data.mongo.collection::<StandingEvent>("events")
        .find(doc! { "_id": doc! {"$gt": first_unreviewed_event_number_num } })
        .sort(doc! { "_id": 1 }).await?;

    while let Some(standing_event) = allegedly_unreviewed.next().await {
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