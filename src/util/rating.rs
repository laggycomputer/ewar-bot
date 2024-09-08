use itertools::Itertools;
use skillratings::trueskill::{trueskill_multi_team, TrueSkillConfig, TrueSkillRating};
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
            10f64 * self.rating
        } else {
            10f64 * (self.rating - self.uncertainty)
        }
    }

    fn format_rating(&self) -> String {
        format!("{:.2}{}", self.leaderboard_rating(), if self.is_provisional() { "**?**" } else { "" })
    }
}

pub(crate) fn game_affect_ratings(placement_system_users: &Vec<TrueSkillRating>) -> Vec<TrueSkillRating> {
    let ratings = placement_system_users.iter()
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