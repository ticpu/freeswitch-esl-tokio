//! The destination readers of `loopback/`, `user/` and `${group_call}` on every built tree.

use freeswitch_c_oracle::{
    oracles, trees_agree, GroupCall, LoopbackOutgoing, UserOutgoing, DEFAULT_DOMAIN,
};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

const PIECES: &[&[u8]] = &[
    b"app=",
    b"APP=",
    b"1000",
    b"echo",
    b"/",
    b"//",
    b":",
    b"@",
    b"+",
    b"F",
    b"A",
    b"E",
    b"sales",
    b"example.com",
    b"ctx",
    b"XML",
    b" ",
    b",",
    b"'",
    b"\\",
    b"\xc3\xa9",
];

fn text() -> impl Strategy<Value = Vec<u8>> {
    vec(select(PIECES), 0..8).prop_map(|pieces| pieces.concat())
}

fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes()
        .to_vec()
}

fn some(text: &str) -> Option<Vec<u8>> {
    Some(bytes(text))
}

fn loopback(
    name: &str,
    destination: &str,
    context: &str,
    dialplan: &str,
    app: bool,
) -> LoopbackOutgoing {
    LoopbackOutgoing {
        name: some(name),
        destination_number: bytes(destination),
        context: some(context),
        dialplan: some(dialplan),
        app,
        variables: Vec::new(),
    }
}

#[test]
fn loopback_splits_extension_context_and_dialplan() {
    let cases: &[(&[u8], LoopbackOutgoing)] = &[
        (
            b"1000",
            loopback("loopback/1000-a", "1000", "default", "xml", false),
        ),
        (
            b"1000/ctx",
            loopback("loopback/1000-a", "1000", "ctx", "xml", false),
        ),
        (
            b"1000/ctx/XML",
            loopback("loopback/1000-a", "1000", "ctx", "XML", false),
        ),
        (
            b"1000//XML",
            loopback("loopback/1000-a", "1000", "default", "XML", false),
        ),
        (
            b"1000/ctx/XML/x",
            loopback("loopback/1000-a", "1000", "ctx", "XML/x", false),
        ),
        (
            b"app=echo",
            LoopbackOutgoing {
                variables: vec![(bytes("loopback_app"), bytes("echo"))],
                ..loopback("loopback/echo-a", "echo", "default", "xml", true)
            },
        ),
        (
            b"APP=playback:tone_stream://x",
            LoopbackOutgoing {
                variables: vec![
                    (bytes("loopback_app"), bytes("playback")),
                    (bytes("loopback_app_arg"), bytes("tone_stream://x")),
                ],
                ..loopback("loopback/playback-a", "playback", "default", "xml", true)
            },
        ),
        (
            b"app=a/b:c",
            LoopbackOutgoing {
                variables: vec![
                    (bytes("loopback_app"), bytes("a/b")),
                    (bytes("loopback_app_arg"), bytes("c")),
                ],
                ..loopback("loopback/a-a", "a", "b", "xml", true)
            },
        ),
    ];
    for (tree, c) in oracles() {
        for (destination, expected) in cases {
            assert_eq!(
                &c.loopback_outgoing_channel(destination),
                expected,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(destination)
            );
        }
    }
}

#[test]
fn loopback_reads_alike_on_every_tree() {
    trees_agree(
        file!(),
        "loopback_reads_alike_on_every_tree",
        text(),
        |c, destination| c.loopback_outgoing_channel(destination),
    );
}

#[test]
fn user_splits_at_the_first_at() {
    let default_domain = String::from_utf8_lossy(DEFAULT_DOMAIN).into_owned();
    let user = |user: &str, domain: &str, default_domain: bool| {
        Some(UserOutgoing {
            user: bytes(user),
            domain: bytes(domain),
            default_domain,
        })
    };
    let cases: &[(&[u8], Option<UserOutgoing>)] = &[
        (b"", None),
        (b"1000@example.com", user("1000", "example.com", false)),
        (b"1000", user("1000", &default_domain, true)),
        (b"a@b@c", user("a", "b@c", false)),
        (b"@example.com", user("", "example.com", false)),
        (b"1000@", user("1000", "", false)),
    ];
    for (tree, c) in oracles() {
        for (destination, expected) in cases {
            assert_eq!(
                &c.user_outgoing_channel(destination),
                expected,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(destination)
            );
        }
    }
}

#[test]
fn user_reads_alike_on_every_tree() {
    trees_agree(
        file!(),
        "user_reads_alike_on_every_tree",
        text(),
        |c, destination| c.user_outgoing_channel(destination),
    );
}

#[test]
fn group_call_takes_the_order_before_the_domain() {
    let default_domain = String::from_utf8_lossy(DEFAULT_DOMAIN).into_owned();
    let group = |group: &str, domain: &str, call_delim: &str, default_domain: bool| {
        Some(GroupCall {
            group: bytes(group),
            domain: some(domain),
            call_delim: bytes(call_delim),
            default_domain,
        })
    };
    let cases: &[(&[u8], Option<GroupCall>)] = &[
        (b"", None),
        (
            b"sales@example.com",
            group("sales", "example.com", ",", false),
        ),
        (b"sales", group("sales", &default_domain, ",", true)),
        (
            b"sales+F@example.com",
            group("sales", &default_domain, "|", true),
        ),
        (
            b"sales@example.com+E",
            group("sales", "example.com", ":_:", false),
        ),
        (
            b"sales@example.com+FA",
            group("sales", "example.com", ",", false),
        ),
        (
            b"sales@example.com+x",
            group("sales", "example.com", ",", false),
        ),
        (b"sales@", group("sales", "", ",", false)),
    ];
    for (tree, c) in oracles() {
        for (arg, expected) in cases {
            assert_eq!(
                &c.group_call(arg),
                expected,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(arg)
            );
        }
    }
}

#[test]
fn group_call_reads_alike_on_every_tree() {
    trees_agree(
        file!(),
        "group_call_reads_alike_on_every_tree",
        text(),
        |c, arg| c.group_call(arg),
    );
}
