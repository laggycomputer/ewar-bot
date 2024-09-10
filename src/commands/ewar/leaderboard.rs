use crate::model::PlayerID;
use crate::{BotError, Context};
use itertools::Itertools;

/// see the highest rated players
#[poise::command(slash_command, prefix_command)]
pub(crate) async fn leaderboard(ctx: Context<'_>) -> Result<(), BotError> {
    let pg_conn = ctx.data().postgres.get().await?;

    // assume sorting by true rating is sufficient (it usually is, except for unusual upsets which can be handled)
    let top = pg_conn.query("SELECT player_id FROM players ORDER BY rating DESC LIMIT 10;", &[]).await?;
    ctx.reply(top.iter()
        .enumerate()
        .map(|(ind, row)| format!("{}. {}", ind + 1, row.get::<&str, PlayerID>("player_id")))
        .join("\n"))
        .await?;

    // TODO
    Ok(())
}