//! `switch_channel_execute_on_value` and the argument `switch_core_session_exec` hands on, on every
//! built tree.

use freeswitch_c_oracle::{oracles, trees_agree, ExecuteOnValue};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

const PIECES: &[&[u8]] = &[
    b" ",
    b":",
    b"::",
    b"'",
    b"\\",
    b"\\'",
    b"\\\\",
    b"\\$",
    b"$",
    b"${",
    b"${a}",
    b"%",
    b"%[",
    b"%[k=v]",
    b"[",
    b"]",
    b"=",
    b",",
    b"lua",
    b"perl",
    b"PerlX",
    b"set",
    b"/run/app/load.lua",
    b"\xc3\xa9",
];

fn text() -> impl Strategy<Value = Vec<u8>> {
    vec(select(PIECES), 0..8).prop_map(|pieces| pieces.concat())
}

fn hook(app: &str, arg: Option<&str>, queued: bool) -> ExecuteOnValue {
    ExecuteOnValue {
        app: app
            .as_bytes()
            .to_vec(),
        arg: arg.map(|arg| {
            arg.as_bytes()
                .to_vec()
        }),
        queued,
        ..ExecuteOnValue::default()
    }
}

#[test]
fn a_hook_value_splits_at_the_first_space_or_single_colon() {
    let cases: &[(&[u8], ExecuteOnValue)] = &[
        (b"answer", hook("answer", None, false)),
        (
            b"set probe=hooked",
            hook("set", Some("probe=hooked"), false),
        ),
        (
            b"set:probe=hooked",
            hook("set", Some("probe=hooked"), false),
        ),
        (b"lua a:b c", hook("lua", Some("a:b c"), false)),
        (b"set ", hook("set", Some(""), false)),
        (b"set:", hook("set", Some(""), false)),
        (b"lua::x y", hook("lua", Some("x y"), true)),
        (b"perl x", hook("perl", Some("x"), true)),
        (b"PerlScript x", hook("PerlScript", Some("x"), true)),
    ];
    for (tree, c) in oracles() {
        for (value, expected) in cases {
            assert_eq!(
                &c.execute_on_value(value),
                expected,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(value)
            );
        }
    }
}

#[test]
fn hook_values_split_alike_on_every_tree() {
    trees_agree(
        file!(),
        "hook_values_split_alike_on_every_tree",
        text(),
        |c, value| c.execute_on_value(value),
    );
}

#[test]
fn an_application_argument_arrives_alike_on_every_tree() {
    trees_agree(
        file!(),
        "an_application_argument_arrives_alike_on_every_tree",
        text(),
        |c, arg| c.exec_argument(arg),
    );
}
