//! The hook's split and the argument reaching its application, against the switch's own C on every
//! built tree.

use freeswitch_c_oracle::against_the_c;
use proptest::option;
use proptest::prelude::*;

use super::split;
use crate::commands::ExecuteOn;
use crate::test_text::text;

/// The port splits a hook's value into the name, argument and queueing the switch reads.
#[test]
fn hook_values_split_as_the_switch_does() {
    against_the_c(
        file!(),
        "hook_values_split_as_the_switch_does",
        text(),
        |c, value| {
            let port = split(&value);
            let switch = c.execute_on_value(value.as_bytes());
            prop_assert_eq!(
                (
                    port.app
                        .as_bytes(),
                    port.arg
                        .map(str::as_bytes),
                    port.queued
                ),
                (
                    &switch.app[..],
                    switch
                        .arg
                        .as_deref(),
                    switch.queued
                ),
                "{:?}",
                value
            );
            Ok(())
        },
    );
}

/// A hook this crate builds hands the switch the name and argument it was given, and the argument
/// reaches the application unexpanded, setting no scope variable.
#[test]
fn an_accepted_hook_reaches_its_application_through_the_c() {
    against_the_c(
        file!(),
        "an_accepted_hook_reaches_its_application_through_the_c",
        (text(), option::of(text())),
        |c, (app, arg)| {
            let Ok(hook) = ExecuteOn::new(app.clone(), arg.clone()) else {
                return Ok(());
            };
            let value = hook.to_string();
            let switch = c.execute_on_value(value.as_bytes());
            prop_assert_eq!(&switch.app[..], app.as_bytes(), "{:?}", value);
            prop_assert_eq!(
                switch
                    .arg
                    .as_deref(),
                arg.as_deref()
                    .map(str::as_bytes),
                "{:?}",
                value
            );
            prop_assert!(
                switch
                    .lookups
                    .is_empty()
                    && switch
                        .api_calls
                        .is_empty(),
                "{:?} looked up {:?}, called {:?}",
                value,
                switch.lookups,
                switch.api_calls
            );
            let Some(arg) = &arg else {
                return Ok(());
            };
            let exec = c.exec_argument(arg.as_bytes());
            prop_assert_eq!(
                exec.argument
                    .as_deref(),
                Some(arg.as_bytes()),
                "{:?}",
                arg
            );
            prop_assert!(
                exec.scope
                    .is_empty()
                    && exec
                        .api_calls
                        .is_empty(),
                "{:?} set {:?}, called {:?}",
                arg,
                exec.scope,
                exec.api_calls
            );
            prop_assert_eq!(
                exec.lookups,
                vec![b"app_disable_expand_variables".to_vec()],
                "{:?}",
                arg
            );
            Ok(())
        },
    );
}
