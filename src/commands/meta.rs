use crate::util::{base_embed, remove_markdown};
use crate::{BotError, Context};
use chrono::Utc;
use gix::ThreadSafeRepository;
use itertools::Itertools;
use poise::CreateReply;

#[poise::command(slash_command, prefix_command)]
pub(crate) async fn ping(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.say(format!("ok, interaction ping {} ms", ctx.ping().await.as_millis().to_string())).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub(crate) async fn git(ctx: Context<'_>) -> Result<(), BotError> {
    let recents = {
        let repo = ThreadSafeRepository::open(".")?.to_thread_local();

        let walk = repo.rev_walk([repo.head_id()?]).first_parent_only().all()?
            .take(6)
            .map(|commit| {
                let commit = commit.unwrap();
                let commit_id = commit.id;
                let found = repo.find_commit(commit_id).unwrap();
                let decoded = found.decode().unwrap();
                (commit_id.to_hex().to_string(), decoded.message().title.to_string(), decoded.author.time.seconds)
            })
            .collect_vec();

        walk
    };

    let mut time_formatter = timeago::Formatter::new();
    time_formatter.num_items(2);

    ctx.send(CreateReply::default()
        .embed(base_embed(ctx)
            .description(recents.into_iter()
                .map(|(hash, message, ts)| {
                    let message = String::from(message.trim());

                    format!("`{}` {} ({})", &hash[..6], remove_markdown(message), time_formatter.convert_chrono(
                        chrono::DateTime::from_timestamp(ts, 0).unwrap(), Utc::now()
                    ))
                })
                .join("\n")
            ))).await?;

    Ok(())
}
