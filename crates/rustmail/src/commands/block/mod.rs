pub mod args;

pub use args::*;

use crate::prelude::commands::*;
use crate::prelude::config::*;
use crate::prelude::db::*;
use crate::prelude::errors::*;
use crate::prelude::i18n::*;
use crate::prelude::utils::*;
use chrono::Utc;
use moka::future::Cache;
use serenity::all::{CommandOptionType, Context, CreateCommand, CreateCommandOption, UserId};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

static BLOCK_NOTICES: LazyLock<Cache<u64, ()>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(10 * 60))
        .build()
});

pub async fn notify_blocked_user(
    ctx: &Context,
    config: &Config,
    user_id: UserId,
    expires_at: Option<i64>,
    force: bool,
) {
    let key = user_id.get();
    if !force && BLOCK_NOTICES.contains_key(&key) {
        return;
    }
    BLOCK_NOTICES.insert(key, ()).await;

    let (message_key, params) = match expires_at {
        Some(ts) => (
            "block.user_notice_temporary",
            HashMap::from([("until".to_string(), format!("<t:{ts}:F>"))]),
        ),
        None => ("block.user_notice_permanent", HashMap::new()),
    };
    let _ = MessageBuilder::system_message(ctx, config)
        .translated_content(message_key, Some(&params), Some(user_id), None)
        .await
        .to_user(user_id)
        .send(false)
        .await;
}

pub struct BlockCommand;

impl Command for BlockCommand {
    type Args = BlockArgs;

    fn name(&self) -> &'static str {
        "block"
    }

    async fn doc(&self, config: &Config) -> String {
        get_translated_message(config, "help.block", None, None, None, None).await
    }

    async fn slash_definitions(&self, config: &Config) -> Vec<CreateCommand> {
        let translate =
            async |key| get_translated_message(config, key, None, None, None, None).await;

        vec![
            CreateCommand::new("block")
                .description(translate("slash_command.block_command_description").await)
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::User,
                        "user",
                        translate("slash_command.block_user_option").await,
                    )
                    .required(false),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "duration",
                        translate("slash_command.block_duration_option").await,
                    )
                    .required(false),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "reason",
                        translate("slash_command.block_reason_option").await,
                    )
                    .required(false),
                )
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::Boolean,
                        "silent",
                        translate("slash_command.block_silent_option").await,
                    )
                    .required(false),
                ),
        ]
    }

    fn parse_text(&self, args: &str) -> ModmailResult<BlockArgs> {
        parse_block_text(args)
    }

    fn parse_slash(&self, options: &SlashOptions) -> ModmailResult<BlockArgs> {
        parse_block_slash(options)
    }

    async fn run(
        &self,
        args: BlockArgs,
        invocation: &Invocation,
        config: &Config,
    ) -> ModmailResult<()> {
        let pool = config
            .db_pool
            .as_ref()
            .ok_or_else(database_connection_failed)?;
        let target = resolve_target(args.user, invocation, config, pool).await?;

        let bot_id = invocation.ctx().cache.current_user().id;
        if target == invocation.user_id() || target == bot_id {
            return Err(translated_validation(invocation, config, "block.invalid_target").await);
        }

        let user_name = match target.to_user(invocation.ctx()).await {
            Ok(user) => user.name,
            Err(_) => target.to_string(),
        };
        let expires_at = args
            .duration
            .map(|d| Utc::now().timestamp().saturating_add(d.as_secs() as i64));

        block_user(
            target,
            &user_name,
            invocation.user_id(),
            expires_at,
            args.reason.as_deref(),
            args.silent,
            pool,
        )
        .await?;

        if !args.silent {
            notify_blocked_user(invocation.ctx(), config, target, expires_at, true).await;
        }

        let mut params = HashMap::from([("user".to_string(), format!("<@{target}>"))]);
        let key = match expires_at {
            Some(ts) => {
                params.insert("until".to_string(), format!("<t:{ts}:F>"));
                "block.success_temporary"
            }
            None => "block.success_permanent",
        };

        let user_id = Some(invocation.user_id());
        let guild_id = invocation.guild_id().map(|g| g.get());
        let mut content =
            get_translated_message(config, key, Some(&params), user_id, guild_id, None).await;
        if let Some(reason) = &args.reason {
            let params = HashMap::from([("reason".to_string(), reason.clone())]);
            let line = get_translated_message(
                config,
                "block.reason_line",
                Some(&params),
                user_id,
                guild_id,
                None,
            )
            .await;
            content = format!("{content}\n{line}");
        }
        if args.silent {
            let line =
                get_translated_message(config, "block.silent_line", None, user_id, guild_id, None)
                    .await;
            content = format!("{content}\n{line}");
        }

        invocation
            .reply(MessageBuilder::system_message(invocation.ctx(), config).content(content))
            .await?;
        Ok(())
    }
}

pub struct UnblockCommand;

impl Command for UnblockCommand {
    type Args = UnblockArgs;

    fn name(&self) -> &'static str {
        "unblock"
    }

    async fn doc(&self, config: &Config) -> String {
        get_translated_message(config, "help.unblock", None, None, None, None).await
    }

    async fn slash_definitions(&self, config: &Config) -> Vec<CreateCommand> {
        let translate =
            async |key| get_translated_message(config, key, None, None, None, None).await;

        vec![
            CreateCommand::new("unblock")
                .description(translate("slash_command.unblock_command_description").await)
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::User,
                        "user",
                        translate("slash_command.unblock_user_option").await,
                    )
                    .required(false),
                ),
        ]
    }

    fn parse_text(&self, args: &str) -> ModmailResult<UnblockArgs> {
        parse_unblock_text(args)
    }

    fn parse_slash(&self, options: &SlashOptions) -> ModmailResult<UnblockArgs> {
        parse_unblock_slash(options)
    }

    async fn run(
        &self,
        args: UnblockArgs,
        invocation: &Invocation,
        config: &Config,
    ) -> ModmailResult<()> {
        let pool = config
            .db_pool
            .as_ref()
            .ok_or_else(database_connection_failed)?;
        let target = resolve_target(args.user, invocation, config, pool).await?;

        let key = if unblock_user(target, pool).await? {
            "unblock.success"
        } else {
            "unblock.not_blocked"
        };

        let params = HashMap::from([("user".to_string(), format!("<@{target}>"))]);
        invocation
            .reply_translated(config, key, Some(&params))
            .await?;
        Ok(())
    }
}

async fn resolve_target(
    user: Option<UserId>,
    invocation: &Invocation,
    config: &Config,
    pool: &SqlitePool,
) -> ModmailResult<UserId> {
    if let Some(user) = user {
        return Ok(user);
    }
    match current_ticket(invocation, pool).await {
        Ok(ticket) if ticket.user_id > 0 => Ok(UserId::new(ticket.user_id as u64)),
        _ => Err(translated_validation(invocation, config, "block.missing_user").await),
    }
}

async fn translated_validation(
    invocation: &Invocation,
    config: &Config,
    key: &str,
) -> ModmailError {
    let message = get_translated_message(
        config,
        key,
        None,
        Some(invocation.user_id()),
        invocation.guild_id().map(|g| g.get()),
        None,
    )
    .await;
    validation_failed(&message)
}
