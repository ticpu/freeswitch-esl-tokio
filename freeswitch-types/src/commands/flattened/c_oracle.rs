//! `switch_channel_str2cause` and its `CAUSE_CHART` against the switch's own C on every built tree.

use freeswitch_c_oracle::{against_the_c, oracles, Oracle};
use proptest::prelude::*;
use proptest::sample::select;

use super::{str2cause, CauseReading};
use crate::channel::HangupCause;
use crate::test_text::text;

/// `NORMAL_CLEARING`, what `switch_channel_str2cause` answers for text it does not recognize.
const UNRECOGNIZED: i32 = 16;

/// Whether the switch reads `text` as the crate's reading says.
fn reads_alike(c: Oracle, text: &str) -> Result<(), TestCaseError> {
    let switch = c.str2cause(text.as_bytes());
    let reading = str2cause(text);
    let agrees = match reading {
        CauseReading::Name(cause) => switch == i32::from(cause.as_number()),
        CauseReading::Number(number) => switch == number as i32,
        CauseReading::Unrecognized => switch == UNRECOGNIZED,
    };
    prop_assert!(
        agrees,
        "{:?} reads {:?}, the switch {}",
        text,
        reading,
        switch
    );
    Ok(())
}

fn random_case(name: &str, flips: &[bool]) -> String {
    name.chars()
        .zip(
            flips
                .iter()
                .cycle(),
        )
        .map(|(c, &flip)| if flip { c.to_ascii_lowercase() } else { c })
        .collect()
}

#[test]
fn every_cause_name_reads_as_its_chart_entry() {
    for (tree, c) in oracles() {
        let chart = c.cause_chart();
        for cause in HangupCause::ALL {
            let entry = chart
                .iter()
                .find(|(name, _)| {
                    name == cause
                        .as_str()
                        .as_bytes()
                });
            assert_eq!(
                entry.map(|(_, number)| *number),
                Some(i32::from(cause.as_number())),
                "{cause} on tree {tree}"
            );
        }
        for (name, number) in &chart {
            let name = String::from_utf8_lossy(name);
            assert_eq!(
                str2cause(&name),
                CauseReading::Name(
                    HangupCause::from_number(*number as u16)
                        .unwrap_or_else(|| panic!("{name} ({number}) on tree {tree}"))
                ),
                "tree {tree}"
            );
        }
    }
}

/// Cause names in any case, digit runs of every length, and text weighted toward the passes.
#[test]
fn cause_text_reads_as_the_switch_reads_it() {
    let names: Vec<&'static str> = HangupCause::ALL
        .iter()
        .map(|cause| cause.as_str())
        .collect();
    let digits = prop_oneof![
        (0u64..1000).prop_map(|n| n.to_string()),
        any::<u64>().prop_map(|n| n.to_string()),
        "[0-9]{1,30}[a-z_ ]{0,3}",
        select(
            &[
                "4294967297",
                "2147483648",
                "9223372036854775808",
                "007",
                "486abc"
            ][..]
        )
        .prop_map(str::to_owned),
    ];
    let cause_text = prop_oneof![
        2 => (select(names), proptest::collection::vec(any::<bool>(), 1..8))
            .prop_map(|(name, flips)| random_case(name, &flips)),
        2 => digits,
        1 => text(),
    ];
    against_the_c(
        file!(),
        "cause_text_reads_as_the_switch_reads_it",
        cause_text,
        |c, text| reads_alike(c, &text),
    );
}
