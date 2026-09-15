//! `inline_dialplan_hunt` and `switch_channel_str2cause` on every built tree.

use freeswitch_c_oracle::{oracles, trees_agree, InlineApplication};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

const PIECES: &[&[u8]] = &[
    b"m:",
    b"m:|:",
    b"m:,:",
    b"m::",
    b"|",
    b",",
    b":",
    b" ",
    b"'",
    b"\"",
    b"\\",
    b"answer",
    b"set",
    b"a=b",
    b"^^",
    b"^^;",
    b";",
    b"{",
    b"}",
    b"USER_BUSY",
    b"user_busy",
    b"17",
    b"486",
    b"-",
    b"NONE",
    b"\xc3\xa9",
];

fn text() -> impl Strategy<Value = Vec<u8>> {
    vec(select(PIECES), 0..8).prop_map(|pieces| pieces.concat())
}

fn app(name: &str, data: Option<&str>) -> InlineApplication {
    (
        name.as_bytes()
            .to_vec(),
        data.map(|data| {
            data.as_bytes()
                .to_vec()
        }),
    )
}

#[test]
fn inline_hunt_splits_on_the_delimiter_then_the_first_colon() {
    let cases: &[(&[u8], Option<Vec<InlineApplication>>)] = &[
        (b"", None),
        (
            b"answer,park",
            Some(vec![app("answer", None), app("park", None)]),
        ),
        (
            b"set:a=b:c, echo",
            Some(vec![app("set", Some("a=b:c")), app("echo", None)]),
        ),
        (
            b"m:|:set:a=1,b|echo",
            Some(vec![app("set", Some("a=1,b")), app("echo", None)]),
        ),
        (b"m:|", Some(vec![app("m", Some("|"))])),
        (b"set:", Some(vec![app("set", Some(""))])),
    ];
    for (tree, c) in oracles() {
        for (target, expected) in cases {
            assert_eq!(
                &c.inline_dialplan_hunt(target),
                expected,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(target)
            );
        }
    }
}

#[test]
fn inline_hunt_reads_alike_on_every_tree() {
    trees_agree(
        file!(),
        "inline_hunt_reads_alike_on_every_tree",
        text(),
        |c, target| c.inline_dialplan_hunt(target),
    );
}

#[test]
fn str2cause_reads_a_digit_run_or_a_name_in_any_case() {
    for (tree, c) in oracles() {
        let cases: &[(&[u8], i32)] = &[
            (b"USER_BUSY", 17),
            (b"user_busy", 17),
            (b"17", 17),
            (b"486abc", 486),
            (b"NONE", 0),
            (b"", 16),
            (b"NOPE", 16),
            (b" 17", 16),
            (b"-1", 16),
            (b"USER_BUSY ", 16),
        ];
        for (text, cause) in cases {
            assert_eq!(
                c.str2cause(text),
                *cause,
                "{:?} on tree {tree}",
                String::from_utf8_lossy(text)
            );
        }
    }
}

#[test]
fn every_chart_name_reads_as_its_first_entry() {
    for (tree, c) in oracles() {
        let chart = c.cause_chart();
        assert_eq!(chart.first(), Some(&(b"NONE".to_vec(), 0)), "tree {tree}");
        for (name, cause) in &chart {
            let first = chart
                .iter()
                .find(|(other, _)| other.eq_ignore_ascii_case(name))
                .map(|(_, first)| *first);
            assert_eq!(
                Some(c.str2cause(name)),
                first,
                "{} ({cause}) on tree {tree}",
                String::from_utf8_lossy(name)
            );
        }
    }
}

#[test]
fn every_tree_shares_one_cause_chart() {
    let mut charts = oracles().map(|(tree, c)| (tree, c.cause_chart()));
    let Some((first, reference)) = charts.next() else {
        return;
    };
    for (tree, chart) in charts {
        assert_eq!(
            chart, reference,
            "CAUSE_CHART on tree {tree} against {first}"
        );
    }
}

#[test]
fn str2cause_reads_alike_on_every_tree() {
    trees_agree(
        file!(),
        "str2cause_reads_alike_on_every_tree",
        text(),
        |c, text| c.str2cause(text),
    );
}
