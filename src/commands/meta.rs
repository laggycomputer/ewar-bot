use crate::util::remove_markdown;
use crate::{BotError, Context};
use gix::ThreadSafeRepository;
use itertools::Itertools;

#[poise::command(slash_command, prefix_command)]
pub(crate) async fn ping(ctx: Context<'_>) -> Result<(), BotError> {
    ctx.say("ok").await?;
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
                let fetched = repo.find_commit(commit_id);
                (commit_id.to_hex().to_string(), remove_markdown(fetched.unwrap().message().unwrap().title.to_string()))
            })
            .collect_vec();

        walk
    };

    ctx.say(format!("{:?}", recents)).await?;

    Ok(())
}
