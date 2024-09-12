use crate::commands::ewar::user::try_lookup_user;
use crate::commands::ewar::user::UserLookupType::SystemID;
use crate::util::paginate::EmbedLinePaginator;
use crate::util::rating::RatingExtra;
use crate::{BotError, Context};

/// see the highest rated players
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn leaderboard(
    ctx: Context<'_>,
    #[description = "Include players with provisional ratings"] include_provisional: Option<bool>,
) -> Result<(), BotError> {
    ctx.defer().await?;

    let pg_conn = ctx.data().postgres.get().await?;

    let lb_rows = pg_conn.query(match include_provisional.unwrap_or(false) {
        true => "\
SELECT player_id,
   CASE
       WHEN deviation > 2.5 THEN 10 * (rating - deviation)\
       ELSE 10 * rating
       END
       AS lb_rating
FROM players
ORDER BY lb_rating DESC;",
        false => "SELECT player_id, 10 * rating AS lb_rating FROM players WHERE deviation <= 2.5 ORDER BY lb_rating DESC;"
    }, &[]).await?;

    if lb_rows.is_empty() {
        ctx.reply("no users in database").await?;
        return Ok(());
    }

    let mut lb_lines = Vec::with_capacity(lb_rows.len());
    for (ind, row) in lb_rows.into_iter().enumerate() {
        let more_data = try_lookup_user(&pg_conn, SystemID(row.get("player_id"))).await?
            .expect("player_id DNE despite being on leaderboard");

        let mut line = format!("{}: {}", more_data.short_summary(), more_data.rating.format_rating());
        line = if more_data.rating.is_provisional() { format!("~~{}~~", line) } else { line };

        lb_lines.push(format!("{}. {}", ind + 1, line).into_boxed_str());
    }

    EmbedLinePaginator::new(lb_lines)
        .run(ctx).await?;

    Ok(())
}