use skillratings::trueskill::{TrueSkillConfig, TrueSkillRating};

pub(crate) static TRUESKILL_CONFIG: TrueSkillConfig = TrueSkillConfig {
    draw_probability: 0f64,
    beta: 2f64,
    // aka tau
    default_dynamics: 0.04,
};

pub(crate) static DEFAULT_RATING: TrueSkillRating = TrueSkillRating {
    rating: 18.0,
    uncertainty: 9.0,
};

pub(crate) static LOG_LIMIT: i64 = 50;
