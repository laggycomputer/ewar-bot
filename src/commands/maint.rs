use crate::{BotError, Context};

#[poise::command(slash_command, prefix_command, owners_only)]
pub(crate) async fn sql(ctx: Context<'_>, query: String) -> Result<(), BotError> {
    let result = ctx.data().postgres.query(&query, &[]).await;
    match result {
        Err(err) => {
            ctx.reply(format!(":x::\n{err}")).await?;
        }
        _ => {}
    }
    Ok(())
}