//! The value of an `execute_on_*` channel variable: an application and its
//! argument.

use std::fmt;

use super::originate::OriginateError;

/// An application for an `execute_on_*` hook such as
/// [`ChannelVariable::ExecuteOnOriginate`](crate::ChannelVariable::ExecuteOnOriginate).
///
/// The switch splits the value at the first space or single colon, expands
/// variables in what follows, and runs the application on the channel. Set as
/// `execute_on_originate` in an originate's block, it runs on the new channel
/// before that channel's session thread starts, so a variable it sets is on the
/// channel before a SIP leg builds its INVITE. That is how a large or free-text
/// value reaches a leg without crossing the dial string's tokenizer: the block
/// carries paths, and the application reads the value from where they point.
/// `docs/dial-string-format.md` has the measurements and the traps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOn {
    app: String,
    arg: Option<String>,
}

impl ExecuteOn {
    /// Refuses an application name the switch would split, an argument it would
    /// expand, and a line break the wire cannot carry.
    pub fn new(
        app: impl Into<String>,
        arg: Option<impl Into<String>>,
    ) -> Result<Self, OriginateError> {
        let app = app.into();
        let arg = arg.map(Into::into);
        if app.is_empty() {
            return Err(OriginateError::ParseError(
                "execute_on application name is empty".into(),
            ));
        }
        if app.contains([' ', ':']) {
            return Err(OriginateError::ParseError(format!(
                "execute_on application name {app:?} carries a space or colon, where the \
                 switch splits the name from its argument"
            )));
        }
        if let Some(arg) = &arg {
            if arg.contains("${") {
                return Err(OriginateError::ParseError(format!(
                    "execute_on argument for {app} carries `${{`, which the switch expands \
                     before the application sees it; pass a path, not content"
                )));
            }
            if arg.contains(['\n', '\r']) {
                return Err(OriginateError::ParseError(format!(
                    "execute_on argument for {app} carries a line break"
                )));
            }
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
        if let Some(word) = words
            .iter()
            .find(|w| w.is_empty() || w.contains(' '))
        {
            return Err(OriginateError::ParseError(format!(
                "lua argument {word:?} is empty or carries a space, on which mod_lua splits \
                 its argument list"
            )));
        }
        Self::new("lua", Some(words.join(" ")))
    }

    /// The application name.
    pub fn app(&self) -> &str {
        &self.app
    }

    /// The argument, as the application receives it after expansion.
    pub fn arg(&self) -> Option<&str> {
        self.arg
            .as_deref()
    }
}

impl fmt::Display for ExecuteOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.arg {
            Some(arg) => write!(f, "{} {}", self.app, arg),
            None => f.write_str(&self.app),
        }
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

    #[test]
    fn lua_refuses_a_path_with_a_space() {
        assert!(ExecuteOn::lua("/run/my app/load.lua", ["/run/doc.xml"]).is_err());
        assert!(ExecuteOn::lua("/run/load.lua", ["/run/my doc.xml"]).is_err());
        assert!(ExecuteOn::lua("/run/load.lua", [""]).is_err());
    }
}
