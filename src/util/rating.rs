use skillratings::trueskill::TrueSkillRating;

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
        format!("{}{}", self.leaderboard_rating(), if self.is_provisional() { "**?**" } else { "" })
    }
}
