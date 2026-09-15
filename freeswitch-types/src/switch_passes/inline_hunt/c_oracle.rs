//! `inline_dialplan_hunt`'s split against the switch's own C on every built tree.

use freeswitch_c_oracle::{against_the_c, Action, Oracle};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use crate::commands::{Application, Endpoint, LoopbackEndpoint, Originate};
use crate::switch_passes::originate_function::c_oracle::APP_NAMES;
use crate::test_text::text;

/// The applications and data `inline_dialplan_hunt` adds for an action list, its split the switch's C.
fn c_hunt(c: Oracle, list: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let (delimiter, list) = match list {
        [b'm', b':', delimiter, b':', rest @ ..] => (*delimiter, rest),
        list => (b',', list),
    };
    c.separate_string(list, delimiter, 128)
        .into_iter()
        .map(|action| {
            let (name, data) = match action
                .iter()
                .position(|&b| b == b':')
            {
                Some(at) => (&action[..at], &action[at + 1..]),
                None => (&action[..], &[][..]),
            };
            let lead = name
                .iter()
                .take_while(|&&b| b == b' ')
                .count();
            (name[lead..].to_vec(), data.to_vec())
        })
        .collect()
}

/// Every separator `inline_with_delimiter` accepts delivers each action as built through
/// `originate_function` and the inline hunt, on blanks and after `^^~`.
#[test]
fn inline_actions_arrive_through_the_c_hunt() {
    let argument = prop_oneof![
        3 => text(),
        1 => select(&["a\nb", "x\ry", "n\tt"][..]).prop_map(str::to_owned),
    ];
    against_the_c(
        file!(),
        "inline_actions_arrive_through_the_c_hunt",
        (
            (0u8..0x80).prop_map(char::from),
            vec((select(APP_NAMES), option::of(argument)), 1..3),
            any::<bool>(),
        ),
        |c, (delimiter, apps, separated)| {
            let endpoint = Endpoint::Loopback(LoopbackEndpoint::new("9199").with_context("test"));
            let built = apps
                .iter()
                .map(|(name, args)| Application::new(*name, args.clone()));
            let Ok(mut originate) = Originate::inline_with_delimiter(endpoint, built, delimiter)
            else {
                return Ok(());
            };
            if separated {
                originate = originate
                    .with_argv_separator('~')
                    .expect("a usable argv separator");
            }
            let line = originate.to_string();
            let api = c.api_originate(
                line.strip_prefix("originate ")
                    .unwrap_or(&line)
                    .as_bytes(),
            );
            let action = api
                .originated
                .and_then(|originated| originated.action);
            let Some(Action::Transfer {
                extension,
                dialplan,
                ..
            }) = action
            else {
                return Err(TestCaseError::fail(format!(
                    "{line:?} transfers nothing: {action:?}"
                )));
            };
            prop_assert_eq!(&dialplan[..], b"inline", "{:?}", line);
            let want: Vec<(Vec<u8>, Vec<u8>)> = apps
                .iter()
                .map(|(name, args)| {
                    (
                        name.as_bytes()
                            .to_vec(),
                        args.clone()
                            .unwrap_or_default()
                            .into_bytes(),
                    )
                })
                .collect();
            prop_assert_eq!(c_hunt(c, &extension), want, "{:?}", line);
            Ok(())
        },
    );
}
