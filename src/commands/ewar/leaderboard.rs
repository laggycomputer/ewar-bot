use crate::model::{AggregatePlayer, Player};
use crate::util::paginate::EmbedLinePaginator;
use crate::util::rating::RatingExtra;
use crate::{BotError, Context};
use bson::doc;
use futures::TryStreamExt;

/// see the highest rated players
#[poise::command(prefix_command, slash_command)]
pub(crate) async fn leaderboard(
    ctx: Context<'_>,
    #[description = "Include players with provisional ratings"] include_provisional: Option<bool>,
) -> Result<(), BotError> {
    ctx.defer().await?;

    let sort_doc = doc! {"$sort": { "lb_rating": -1 }};
    let aggregate_players = ctx.data().mongo.collection::<Player>("players").aggregate(if include_provisional.unwrap_or(false) {
        let agg_doc = doc! {
            "$project": {
                "lb_rating": {
                    "$cond": {
                        "if": {"$gt": ["$deviation", 2.5]},
                        "then": {"$multiply": [{"$subtract": ["$rating", "$deviation"]}, 10]},
                        "else": {"$multiply": ["$rating", 10]},
                    }
                },
                "inner": "$$ROOT"
            }
        };
        vec![agg_doc, sort_doc]
    } else {
        let filter_doc = doc! {
            "$match": {"deviation": {"$lte": 2.5}}
        };
        let agg_doc = doc! {
            "$project": {
                "lb_rating": {"$multiply": ["$rating", 10]},
                "inner": "$$ROOT"
            }
        };
        vec![filter_doc, agg_doc, sort_doc]
    }).with_type::<AggregatePlayer>().await?
        .try_collect::<Vec<_>>().await?;

    if aggregate_players.is_empty() {
        ctx.reply("no users found").await?;
        return Ok(());
    }

    let mut lb_lines = Vec::with_capacity(aggregate_players.len());
    for (ind, agg_player) in aggregate_players.into_iter().enumerate() {
        let mut line = format!("{}: {}", agg_player.inner.short_summary(), agg_player.inner.rating_struct().format_rating());
        line = if agg_player.inner.rating_struct().is_provisional() { format!("~~{}~~", line) } else { line };

        lb_lines.push(format!("{}. {}", ind + 1, line).into_boxed_str());
    }

    EmbedLinePaginator::new(lb_lines)
        .run(ctx).await?;

    Ok(())
}