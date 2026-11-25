use crate::prelude::commands::*;
use crate::prelude::errors::*;
use crate::prelude::utils::*;
use serenity::all::UserId;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockArgs {
    pub user: Option<UserId>,
    pub duration: Option<Duration>,
    pub reason: Option<String>,
    pub silent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnblockArgs {
    pub user: Option<UserId>,
}

pub fn parse_block_text(args: &str) -> ModmailResult<BlockArgs> {
    let mut silent = false;
    let mut tokens: &[&str] = &args
        .split_whitespace()
        .filter(|token| {
            let flag = token.eq_ignore_ascii_case("-s") || token.eq_ignore_ascii_case("--silent");
            silent |= flag;
            !flag
        })
        .collect::<Vec<_>>();

    let mut user = None;
    if let Some((first, rest)) = tokens.split_first()
        && let Some(id) = user_token(first)
    {
        user = Some(id);
        tokens = rest;
    }

    let mut duration = None;
    if let Some((first, rest)) = tokens.split_first()
        && let Some(parsed) = duration_token(first)
    {
        duration = Some(parsed);
        tokens = rest;
    }

    Ok(BlockArgs {
        user,
        duration,
        reason: non_empty(&tokens.join(" ")),
        silent,
    })
}

pub fn parse_block_slash(options: &SlashOptions) -> ModmailResult<BlockArgs> {
    let duration = match options.string("duration").map(str::trim) {
        Some(spec) if !spec.is_empty() => Some(duration_token(spec).ok_or_else(|| {
            ModmailError::Command(CommandError::InvalidArguments(spec.to_string()))
        })?),
        _ => None,
    };

    Ok(BlockArgs {
        user: options.user("user"),
        duration,
        reason: options.string("reason").and_then(non_empty),
        silent: options.boolean("silent").unwrap_or(false),
    })
}

pub fn parse_unblock_text(args: &str) -> ModmailResult<UnblockArgs> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(UnblockArgs { user: None });
    }
    parse_user_id(args)
        .map(|id| UnblockArgs { user: Some(id) })
        .ok_or_else(|| ModmailError::Command(CommandError::InvalidArguments(args.to_string())))
}

pub fn parse_unblock_slash(options: &SlashOptions) -> ModmailResult<UnblockArgs> {
    Ok(UnblockArgs {
        user: options.user("user"),
    })
}

fn user_token(token: &str) -> Option<UserId> {
    let is_snowflake = token.len() >= 15 && token.chars().all(|c| c.is_ascii_digit());
    if token.starts_with("<@") || is_snowflake {
        parse_user_id(token)
    } else {
        None
    }
}

fn duration_token(token: &str) -> Option<Duration> {
    if !token.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    parse_duration_spec(token)
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: u64 = 123_456_789_012_345_678;

    fn args(user: Option<u64>, secs: Option<u64>, reason: Option<&str>) -> BlockArgs {
        BlockArgs {
            user: user.map(UserId::new),
            duration: secs.map(Duration::from_secs),
            reason: reason.map(str::to_string),
            silent: false,
        }
    }

    fn silent(args: BlockArgs) -> BlockArgs {
        BlockArgs {
            silent: true,
            ..args
        }
    }

    #[test]
    fn text_without_arguments_targets_ticket_permanently() {
        assert_eq!(parse_block_text("").unwrap(), args(None, None, None));
    }

    #[test]
    fn text_user_duration_and_reason() {
        assert_eq!(
            parse_block_text(&format!("<@{ID}> 7d spam in DMs")).unwrap(),
            args(Some(ID), Some(7 * 86400), Some("spam in DMs"))
        );
        assert_eq!(
            parse_block_text(&format!("{ID} 1h30m")).unwrap(),
            args(Some(ID), Some(5400), None)
        );
    }

    #[test]
    fn text_in_ticket_duration_or_reason_only() {
        assert_eq!(
            parse_block_text("2h insults").unwrap(),
            args(None, Some(7200), Some("insults"))
        );
        assert_eq!(
            parse_block_text("spam").unwrap(),
            args(None, None, Some("spam"))
        );
    }

    #[test]
    fn short_numbers_stay_in_the_reason() {
        assert_eq!(
            parse_block_text("5 insults").unwrap(),
            args(None, None, Some("5 insults"))
        );
    }

    #[test]
    fn text_silent_flag_anywhere() {
        assert_eq!(
            parse_block_text("-s").unwrap(),
            silent(args(None, None, None))
        );
        assert_eq!(
            parse_block_text("7d spam -s").unwrap(),
            silent(args(None, Some(7 * 86400), Some("spam")))
        );
        assert_eq!(
            parse_block_text(&format!("--SILENT <@{ID}> 1h raid")).unwrap(),
            silent(args(Some(ID), Some(3600), Some("raid")))
        );
    }

    #[test]
    fn slash_options() {
        let options = SlashOptions::default()
            .with("user", OptionValue::User(UserId::new(ID)))
            .with("duration", OptionValue::String("30m".into()))
            .with("reason", OptionValue::String("  raid ".into()))
            .with("silent", OptionValue::Boolean(true));
        assert_eq!(
            parse_block_slash(&options).unwrap(),
            silent(args(Some(ID), Some(1800), Some("raid")))
        );
        assert_eq!(
            parse_block_slash(&SlashOptions::default()).unwrap(),
            args(None, None, None)
        );
    }

    #[test]
    fn slash_rejects_invalid_duration() {
        let options = SlashOptions::default().with("duration", OptionValue::String("soon".into()));
        assert!(parse_block_slash(&options).is_err());
    }

    #[test]
    fn unblock_text() {
        assert_eq!(parse_unblock_text("").unwrap().user, None);
        assert_eq!(
            parse_unblock_text(&format!("<@!{ID}>")).unwrap().user,
            Some(UserId::new(ID))
        );
        assert!(parse_unblock_text("bob").is_err());
    }
}
