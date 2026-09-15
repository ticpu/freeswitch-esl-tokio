//! The value of an `execute_on_*` channel variable: an application and its
//! argument.

use std::fmt;

use crate::switch_passes::execute_on::{expansion_rewrites, opens_scope_variables, render, split};

use super::originate::OriginateError;

/// The part of a value a fault names.
const APPLICATION: &str = "application name";
const ARGUMENT: &str = "argument";

/// What the switch does with an `execute_on_*` application name or argument that keeps it from
/// reaching the application as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecuteOnFault {
    /// Empty, which names no application.
    Empty,
    /// Carries a space, a lone colon or a NUL, where the switch cuts the value.
    Cut,
    /// Carries a line break, which no command line holds.
    LineBreak,
    /// Carries an escape or a reference the expansion reads before the application does.
    Expanded,
    /// Opens a block of scope variables, which the application never receives.
    OpensScopeVariables,
}

impl fmt::Display for ExecuteOnFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("is empty, which names no application"),
            Self::Cut => f.write_str(
                "carries a space, a lone colon or a NUL, where the switch cuts the value",
            ),
            Self::LineBreak => f.write_str("carries a line break, which no command line holds"),
            Self::Expanded => f.write_str(
                "carries an escape or a reference the expansion reads before the \
                             application does",
            ),
            Self::OpensScopeVariables => f.write_str(
                "opens a block of scope variables, which the application never receives",
            ),
        }
    }
}

/// An application for an `execute_on_*` hook such as
/// [`ChannelVariable::ExecuteOnOriginate`](crate::ChannelVariable::ExecuteOnOriginate).
///
/// The switch splits the value at the first space or lone colon, expands what follows and runs the
/// application on the channel; a name opening `perl` in any case, or one carrying `::`, is queued
/// on the session instead of run where the hook fires. Only a name and argument the application
/// receives as written are representable. Set as `execute_on_originate` in an originate's block,
/// the hook runs on the new channel before that channel's session thread starts, so a variable it
/// sets is on the channel before a SIP leg builds its INVITE. That is how a large or free-text
/// value reaches a leg without crossing the dial string's tokenizer: the block carries paths, and
/// the application reads the value from where they point. Not on a loopback leg, which the hook
/// wedges in `CS_INIT`. `docs/dial-string-format.md` has the measurements and the traps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOn {
    app: String,
    arg: Option<String>,
}

impl ExecuteOn {
    /// Refuses a name or argument the switch cuts, expands or reads as a block of scope variables,
    /// and a line break the wire cannot carry.
    pub fn new(
        app: impl Into<String>,
        arg: Option<impl Into<String>>,
    ) -> Result<Self, OriginateError> {
        let app = app.into();
        let arg = arg.map(Into::into);
        if app.is_empty() {
            return Err(undeliverable(APPLICATION, ExecuteOnFault::Empty));
        }
        if app.contains(['\n', '\r']) {
            return Err(undeliverable(APPLICATION, ExecuteOnFault::LineBreak));
        }
        if let Some(arg) = &arg {
            if arg.contains(['\n', '\r']) {
                return Err(undeliverable(ARGUMENT, ExecuteOnFault::LineBreak));
            }
            if opens_scope_variables(arg) {
                return Err(undeliverable(ARGUMENT, ExecuteOnFault::OpensScopeVariables));
            }
            if expansion_rewrites(arg) {
                return Err(undeliverable(ARGUMENT, ExecuteOnFault::Expanded));
            }
        }
        let value = render(&app, arg.as_deref());
        let hook = split(&value);
        if hook.app != app {
            return Err(undeliverable(APPLICATION, ExecuteOnFault::Cut));
        }
        if hook.arg != arg.as_deref() {
            return Err(undeliverable(ARGUMENT, ExecuteOnFault::Cut));
        }
        Ok(Self { app, arg })
    }

    /// `lua <script> <args…>`. mod_lua splits its argument on spaces, so a
    /// script path or argument carrying one is refused.
    pub fn lua(
        script: impl AsRef<str>,
        args: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Self, OriginateError> {
        let mut words = vec![script
            .as_ref()
            .to_string()];
        words.extend(
            args.into_iter()
                .map(|a| {
                    a.as_ref()
                        .to_string()
                }),
        );
        if words
            .iter()
            .any(|word| word.is_empty() || word.contains(' '))
        {
            return Err(OriginateError::ParseError(
                "a lua argument is empty or carries a space, on which mod_lua splits its \
                 argument list"
                    .to_owned(),
            ));
        }
        Self::new("lua", Some(words.join(" ")))
    }

    /// The application name.
    pub fn app(&self) -> &str {
        &self.app
    }

    /// The argument, as the application receives it.
    pub fn arg(&self) -> Option<&str> {
        self.arg
            .as_deref()
    }
}

fn undeliverable(part: &'static str, fault: ExecuteOnFault) -> OriginateError {
    OriginateError::UndeliverableExecuteOn { part, fault }
}

impl fmt::Display for ExecuteOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render(
            &self.app,
            self.arg
                .as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_app_and_argument() {
        let hook = ExecuteOn::new("set", Some("probe=hooked")).unwrap();
        assert_eq!(hook.to_string(), "set probe=hooked");
        assert_eq!(hook.app(), "set");
        assert_eq!(hook.arg(), Some("probe=hooked"));

        let bare = ExecuteOn::new("answer", None::<String>).unwrap();
        assert_eq!(bare.to_string(), "answer");
        assert_eq!(bare.arg(), None);
    }

    #[test]
    fn lua_joins_script_and_arguments() {
        let hook = ExecuteOn::lua("/run/app/load.lua", ["/run/app/doc.xml", "pidf"]).unwrap();
        assert_eq!(
            hook.to_string(),
            "lua /run/app/load.lua /run/app/doc.xml pidf"
        );
    }

    /// The switch splits the name from the argument at the first space or
    /// single colon, so a name carrying either names a different application.
    #[test]
    fn refuses_a_name_the_switch_would_split() {
        assert!(ExecuteOn::new("lua:x", None::<String>).is_err());
        assert!(ExecuteOn::new("my app", None::<String>).is_err());
        assert!(ExecuteOn::new("", None::<String>).is_err());
    }

    #[test]
    fn refuses_an_argument_the_switch_would_expand_or_the_wire_cannot_carry() {
        assert!(ExecuteOn::new("set", Some("k=${v}")).is_err());
        assert!(ExecuteOn::new("set", Some("k=a\nb")).is_err());
    }

    /// `switch_core_session_exec` expands the argument, and that pass drops the backslash of `\\`
    /// and `\$` and the whole of `\'`. It runs only on a value carrying `\\`, `\n`, `\s`, `\t`,
    /// `\'` or a reference, so a `\$` alone rides through and one beside such an escape does not.
    #[test]
    fn refuses_an_argument_the_expansion_unescapes() {
        assert!(ExecuteOn::new("set", Some(r"k=a\\b")).is_err());
        assert!(ExecuteOn::new("set", Some(r"k=it\'s")).is_err());
        assert!(ExecuteOn::new("set", Some(r"k=\$10\\")).is_err());
        assert!(ExecuteOn::new("set", Some(r"k=\$10")).is_ok());
    }

    /// An expanded argument opening `%[` or `%<delim>[` is read as a block of scope variables and
    /// never reaches the application; a bare `%` leaves the switch reading past the terminator.
    #[test]
    fn refuses_an_argument_that_opens_scope_variables() {
        assert!(ExecuteOn::new("set", Some("%[k=v]rest")).is_err());
        assert!(ExecuteOn::new("set", Some("%|[k=v]rest")).is_err());
        assert!(ExecuteOn::new("set", Some("%")).is_err());
    }

    /// The name rides the same wire as the argument, and a NUL ends what the switch reads of
    /// either.
    #[test]
    fn refuses_a_name_or_argument_the_wire_cuts() {
        assert!(ExecuteOn::new("se\nt", None::<String>).is_err());
        assert!(ExecuteOn::new("se\0t", None::<String>).is_err());
        assert!(ExecuteOn::new("set", Some("k=a\0b")).is_err());
    }

    #[test]
    fn lua_refuses_a_path_with_a_space() {
        assert!(ExecuteOn::lua("/run/my app/load.lua", ["/run/doc.xml"]).is_err());
        assert!(ExecuteOn::lua("/run/load.lua", ["/run/my doc.xml"]).is_err());
        assert!(ExecuteOn::lua("/run/load.lua", [""]).is_err());
    }

    /// A name the switch queues on the session is still the name it runs.
    #[test]
    fn a_perl_name_is_representable() {
        let hook = ExecuteOn::new("perl", Some("/run/app/load.pl")).unwrap();
        assert_eq!(hook.to_string(), "perl /run/app/load.pl");
    }
}
