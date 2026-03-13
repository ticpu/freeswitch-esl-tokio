//! FreeSWITCH dptools application commands (`answer`, `hangup`, `playback`, etc.).

use crate::command::EslCommand;

/// Constructors for common dptools application commands.
///
/// Each method returns a `sendmsg`/`execute` command ready for
/// [`EslClient::send_command()`](crate::EslClient::send_command).
/// The `uuid` field is `None` — set it on the command or pass it to `sendmsg()`.
pub struct AppCommand;

impl AppCommand {
    /// Answer the channel. In outbound mode, answers the incoming call.
    pub fn answer() -> EslCommand {
        EslCommand::Execute {
            app: "answer".to_string(),
            args: None,
            uuid: None,
        }
    }

    /// `cause`: hangup cause string (e.g. `NORMAL_CLEARING`). `None` uses default.
    pub fn hangup(cause: Option<&str>) -> EslCommand {
        EslCommand::Execute {
            app: "hangup".to_string(),
            args: cause.map(|c| c.to_string()),
            uuid: None,
        }
    }

    /// `file`: path, `tone_stream://`, or any FreeSWITCH file-like URI.
    pub fn playback(file: &str) -> EslCommand {
        EslCommand::Execute {
            app: "playback".to_string(),
            args: Some(file.to_string()),
            uuid: None,
        }
    }

    /// `destination`: dial string for the B-leg (e.g. `sofia/gateway/gw/number`).
    pub fn bridge(destination: &str) -> EslCommand {
        EslCommand::Execute {
            app: "bridge".to_string(),
            args: Some(destination.to_string()),
            uuid: None,
        }
    }

    /// Set a channel variable (`set` application).
    pub fn set_var(name: &str, value: &str) -> EslCommand {
        EslCommand::Execute {
            app: "set".to_string(),
            args: Some(format!("{}={}", name, value)),
            uuid: None,
        }
    }

    /// Park the channel (suspends dialplan execution until another command picks it up).
    pub fn park() -> EslCommand {
        EslCommand::Execute {
            app: "park".to_string(),
            args: None,
            uuid: None,
        }
    }

    /// Transfer the channel to another dialplan extension.
    ///
    /// FreeSWITCH `transfer` syntax is positional: `<extension> [<dialplan> [<context>]]`.
    /// When `context` is set but `dialplan` is `None`, the dialplan slot is filled
    /// with `"XML"` (the FreeSWITCH default) so that the context is not misread
    /// as the dialplan argument.
    pub fn transfer(extension: &str, dialplan: Option<&str>, context: Option<&str>) -> EslCommand {
        let mut args = extension.to_string();
        if let Some(dp) = dialplan {
            args.push(' ');
            args.push_str(dp);
        } else if context.is_some() {
            args.push_str(" XML");
        }
        if let Some(ctx) = context {
            args.push(' ');
            args.push_str(ctx);
        }

        EslCommand::Execute {
            app: "transfer".to_string(),
            args: Some(args),
            uuid: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_context_without_dialplan_fills_xml_gap() {
        let cmd = AppCommand::transfer("1000", None, Some("default"));
        match cmd {
            EslCommand::Execute { args, .. } => {
                assert_eq!(args.as_deref(), Some("1000 XML default"));
            }
            _ => panic!("expected Execute"),
        }
    }

    #[test]
    fn transfer_with_both_dialplan_and_context() {
        let cmd = AppCommand::transfer("1000", Some("XML"), Some("default"));
        match cmd {
            EslCommand::Execute { args, .. } => {
                assert_eq!(args.as_deref(), Some("1000 XML default"));
            }
            _ => panic!("expected Execute"),
        }
    }

    #[test]
    fn transfer_extension_only() {
        let cmd = AppCommand::transfer("1000", None, None);
        match cmd {
            EslCommand::Execute { args, .. } => {
                assert_eq!(args.as_deref(), Some("1000"));
            }
            _ => panic!("expected Execute"),
        }
    }
}
