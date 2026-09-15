use super::*;
use crate::commands::variables::DialStringCarrier;
use crate::variables::ChannelVariable;

const API: DialStringCarrier = DialStringCarrier::EslApi;
const DIALPLAN: DialStringCarrier = DialStringCarrier::Dialplan;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../../tests/fixtures/flattened/", $name, ".txt"))
    };
}

fn parse(input: &str, carrier: DialStringCarrier) -> FlattenedDialString {
    FlattenedDialString::parse_for(input, carrier)
        .unwrap_or_else(|e| panic!("{input:?} at {carrier:?}: {e:?}"))
}

fn presence(leg: &FlattenedLeg) -> Option<&str> {
    leg.variable(ChannelVariable::PresenceId)
}

fn is_error(leg: &FlattenedLeg) -> bool {
    matches!(leg.target(), LegTarget::Error(_))
}

#[test]
fn raw_render_is_the_input_for_every_fixture() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/flattened");
    let mut parsed = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry
            .unwrap()
            .path();
        if path
            .extension()
            .is_none_or(|ext| ext != "txt")
        {
            continue;
        }
        let input = std::fs::read_to_string(&path).unwrap();
        for carrier in [API, DIALPLAN] {
            if let Ok(list) = FlattenedDialString::parse_for(&input, carrier) {
                parsed += 1;
                assert_eq!(
                    list.display_raw()
                        .to_string(),
                    input,
                    "{path:?} at {carrier:?}"
                );
            }
        }
    }
    assert!(parsed > 100, "{parsed}");
}

#[test]
fn plain_fixtures_render_back_to_the_input() {
    for input in [
        fixture!("g-fp-static.A"),
        fixture!("g-fp-reg.A"),
        fixture!("pbx-calltakers.A"),
    ] {
        assert_eq!(
            parse(input, API)
                .display_for(API)
                .to_string(),
            input
        );
    }
}

#[test]
fn head_blocks_render_ahead_and_only_set_pairs_are_carried() {
    let list = parse(
        "<e=1>{g=2}[k,a=1]loopback/9199/test:_:{h=3}loopback/9199/x|[b=2]null/a",
        API,
    );
    assert_eq!(
        list.display_for(API)
            .to_string(),
        "<e=1>{g=2}[a=1]loopback/9199/test:_:{h=3}loopback/9199/x|[b=2]null/a"
    );
}

#[test]
fn dropping_error_legs_rejoins_the_rest() {
    for input in [fixture!("flattened-probe.A"), fixture!("pbx-calltakers.A")] {
        let mut list = parse(input, API);
        assert!(list
            .legs()
            .any(is_error));
        list.retain(|leg| !is_error(leg));
        let want = input
            .split(',')
            .filter(|leg| !leg.contains("]error/"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            list.display_raw()
                .to_string(),
            want
        );
        assert!(!list
            .legs()
            .any(is_error));
    }
}

#[test]
fn a_leg_ending_in_an_escaped_char_keeps_it_after_retain() {
    let cases = [
        (API, r"loopback/9199/test\\s"),
        (API, r"loopback/9199/test\\\\\\\\"),
        (DIALPLAN, r"loopback/9199/test\\\\\\\\"),
        (DIALPLAN, r"loopback/9199/test\s"),
    ];
    for (carrier, kept) in cases {
        let input = format!("{kept},error/USER_BUSY");
        let mut list = parse(&input, carrier);
        list.retain(|leg| !is_error(leg));
        assert_eq!(
            list.display_raw()
                .to_string(),
            kept,
            "{input:?} at {carrier:?}"
        );
    }
}

/// The switch splits on the first byte of a non-ASCII `^^` separator, which no char
/// delimiter mirrors, so the blank split runs over the whole argument.
#[test]
fn a_non_ascii_argument_separator_is_not_taken() {
    assert_eq!(
        FlattenedDialString::parse_for("^^é{v=a b}loopback/9199/test", API),
        Err(FlattenedDialStringError::ArgvSplit)
    );
}

#[test]
fn a_thread_ending_in_an_escaped_quote_keeps_it_after_retain() {
    let kept = r"loopback/9199/test\'";
    let input = format!("{kept}:_:error/USER_BUSY");
    let mut list = parse(&input, API);
    list.retain(|leg| !is_error(leg));
    assert_eq!(
        list.display_raw()
            .to_string(),
        kept
    );
}

#[test]
fn an_edge_leg_takes_its_one_separator() {
    let input = fixture!("g-fp-reg.A");
    let (first, last) = input
        .split_once(',')
        .unwrap();

    let mut list = parse(input, API);
    list.retain(|leg| {
        !leg.raw()
            .contains("gw=fp-reg-b")
    });
    assert_eq!(
        list.display_raw()
            .to_string(),
        last
    );

    let mut list = parse(input, API);
    list.retain(|leg| {
        !leg.raw()
            .contains("gw=fp-reg-a")
    });
    assert_eq!(
        list.display_raw()
            .to_string(),
        first
    );
    assert_eq!(
        list.legs()
            .map(FlattenedLeg::raw)
            .collect::<Vec<_>>(),
        [first]
    );
}

#[test]
fn an_emptied_group_goes_with_its_separator() {
    let input = fixture!("g-fp-pipe.A");
    let (first, rest) = input
        .split_once('|')
        .unwrap();
    let mut list = parse(input, API);
    let mut seen = 0;
    list.retain(|_| {
        seen += 1;
        seen != 1
    });
    assert_eq!(
        list.threads()
            .flat_map(FlattenedThread::groups)
            .count(),
        1
    );
    assert_eq!(
        list.display_raw()
            .to_string(),
        rest
    );

    let mut list = parse(input, API);
    let mut seen = 0;
    list.retain(|_| {
        seen += 1;
        seen != 2
    });
    let (_, last) = rest
        .split_once(',')
        .unwrap();
    assert_eq!(
        list.display_raw()
            .to_string(),
        format!("{first}|{last}")
    );
}

#[test]
fn an_emptied_thread_goes_with_its_separator() {
    let input = fixture!("g-fp-ent.none");
    let (first, rest) = input
        .split_once(":_:")
        .unwrap();

    let mut list = parse(input, API);
    list.retain(|leg| presence(leg) != Some("fp-ent@pbx.example.com"));
    assert_eq!(
        list.threads()
            .count(),
        1
    );
    assert_eq!(
        list.display_raw()
            .to_string(),
        rest
    );

    let mut list = parse(input, API);
    list.retain(|leg| presence(leg) == Some("fp-ent@pbx.example.com"));
    assert_eq!(
        list.display_raw()
            .to_string(),
        first
    );
}

#[test]
fn removing_every_leg_leaves_nothing_to_render() {
    let mut list = parse(fixture!("flattened-probe.A"), API);
    assert!(!list.is_empty());
    list.retain(|_| false);
    assert!(list.is_empty());
    assert_eq!(
        list.legs()
            .count(),
        0
    );
    assert_eq!(
        list.display_raw()
            .to_string(),
        ""
    );
    assert_eq!(
        list.display_for(API)
            .to_string(),
        ""
    );
}

#[test]
fn presence_id_is_read_through_every_scope() {
    let list = parse(fixture!("g-fp-static.A"), API);
    assert_eq!(
        list.legs()
            .map(presence)
            .collect::<Vec<_>>(),
        [Some("fp-static@pbx.example.com")]
    );

    let list = parse(fixture!("g-fp-nodial.A"), API);
    assert_eq!(
        list.legs()
            .map(presence)
            .collect::<Vec<_>>(),
        [None]
    );

    let list = parse(
        "<presence_id=e>loopback/9199/test:_:{presence_id=g}[presence_id=l]null/a",
        API,
    );
    assert_eq!(
        list.legs()
            .map(presence)
            .collect::<Vec<_>>(),
        [Some("e"), Some("g")]
    );
}

#[test]
fn leg_targets_are_typed() {
    let list = parse(fixture!("fp-one-empty.A"), API);
    let targets: Vec<&LegTarget> = list
        .legs()
        .map(FlattenedLeg::target)
        .collect();
    assert!(matches!(
        targets[0],
        LegTarget::Endpoint(Endpoint::Loopback(_))
    ));
    match targets[1] {
        LegTarget::Unparsed(unparsed) => assert_eq!(unparsed.endpoint(), ""),
        other => panic!("{other:?}"),
    }
    assert!(matches!(targets[2], LegTarget::Error(_)));

    let list = parse(fixture!("g-fp-nodial.A"), API);
    assert!(matches!(
        list.legs()
            .next()
            .map(FlattenedLeg::target),
        Some(LegTarget::Endpoint(Endpoint::User(_)))
    ));
}

fn error_leg(list: &FlattenedDialString) -> &ErrorLeg {
    match list
        .legs()
        .next()
        .map(FlattenedLeg::target)
    {
        Some(LegTarget::Error(error)) => error,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_cause_fixture_is_read_as_the_switch_reads_it() {
    for (input, written, reading, cause) in [
        (
            fixture!("g-fp-err-bogus.A"),
            "NOT_A_CAUSE",
            CauseReading::Unrecognized,
            None,
        ),
        (
            fixture!("g-fp-err-empty.A"),
            "",
            CauseReading::Unrecognized,
            None,
        ),
        (
            fixture!("g-fp-err-lower.A"),
            "user_busy",
            CauseReading::Name(HangupCause::UserBusy),
            Some(HangupCause::UserBusy),
        ),
        (
            fixture!("g-fp-err-upper.A"),
            "USER_BUSY",
            CauseReading::Name(HangupCause::UserBusy),
            Some(HangupCause::UserBusy),
        ),
        (
            fixture!("g-fp-err-num.A"),
            "17",
            CauseReading::Number(17),
            Some(HangupCause::UserBusy),
        ),
        (
            fixture!("g-fp-err-prefix.A"),
            "17abc",
            CauseReading::Number(17),
            Some(HangupCause::UserBusy),
        ),
        (
            fixture!("g-fp-err-zero.A"),
            "0",
            CauseReading::Number(0),
            Some(HangupCause::None),
        ),
        (
            fixture!("g-fp-unreg.A"),
            "user_not_registered",
            CauseReading::Name(HangupCause::UserNotRegistered),
            Some(HangupCause::UserNotRegistered),
        ),
        (
            fixture!("fp-none.A"),
            "NO_ROUTE_DESTINATION",
            CauseReading::Name(HangupCause::NoRouteDestination),
            Some(HangupCause::NoRouteDestination),
        ),
        ("error/70000", "70000", CauseReading::Number(70000), None),
        ("error/4", "4", CauseReading::Number(4), None),
    ] {
        for carrier in [API, DIALPLAN] {
            let list = parse(input, carrier);
            let error = error_leg(&list);
            assert_eq!(error.as_written(), written, "{input}");
            assert_eq!(error.reading(), reading, "{input}");
            assert_eq!(error.cause(), cause, "{input}");
        }
    }
}

#[test]
fn list_warnings_are_raised() {
    assert_eq!(parse(fixture!("g-fp-static.A"), DIALPLAN).warnings(), []);
    assert_eq!(
        parse("loopback/9199/'a|b'", DIALPLAN).warnings(),
        [ListWarning::QuoteSpansLegs]
    );
    assert_eq!(
        parse("[k=${x}]loopback/9199/test", DIALPLAN).warnings(),
        [ListWarning::CarrierExpands]
    );
}

fn leg_warnings(input: &str, carrier: DialStringCarrier) -> Vec<LegWarning> {
    parse(input, carrier)
        .legs()
        .flat_map(|leg| {
            leg.warnings()
                .to_vec()
        })
        .collect()
}

#[test]
fn leg_warnings_name_block_and_key() {
    assert_eq!(
        leg_warnings("[a=1][k]loopback/9199/test", API),
        [LegWarning::PairIgnored {
            block: 1,
            key: "k".into()
        }]
    );
    assert_eq!(
        leg_warnings(
            r"[k=1][sentinel=s,k=\\\\\\\\\\\\\\'\\\\\\\\\\\\\\']loopback/9199/test",
            DIALPLAN
        ),
        [LegWarning::PairCleared {
            block: 1,
            key: "k".into()
        }]
    );
    assert_eq!(
        leg_warnings(fixture!("g-fp-esc-nested.A"), API),
        [LegWarning::NestedVarsRefused {
            block: 0,
            key: "nv".into()
        }]
    );
    assert_eq!(
        leg_warnings("{origination_nested_vars=true}[nv=${x}]null/a", API),
        []
    );
    assert_eq!(leg_warnings(fixture!("g-fp-reg.A"), API), []);
}

#[test]
fn errors_name_their_kind_and_never_quote_input() {
    for (input, carrier, want) in [
        ("", API, FlattenedDialStringError::Empty),
        (
            fixture!("g-fp-esc-space.A"),
            API,
            FlattenedDialStringError::ArgvSplit,
        ),
        (
            fixture!("g-fp-esc-pipe.A"),
            API,
            FlattenedDialStringError::UnclosedBlock { leg: 0 },
        ),
        (
            "[presence_id=fp@pbx.example.com]loopback/9199/test\n",
            API,
            FlattenedDialStringError::TrailingNewline,
        ),
    ] {
        let err = FlattenedDialString::parse_for(input, carrier).unwrap_err();
        assert_eq!(err, want, "{input:?}");
        let shown = err.to_string();
        assert!(!shown.is_empty());
        for fragment in ["presence", "example", "loopback", "9199"] {
            assert!(!shown.contains(fragment), "{shown}");
        }
    }
}

#[test]
fn warnings_and_unparsed_legs_never_quote_values() {
    let list = parse("[nv=${secret_ref},k=]bogus/secret-endpoint", API);
    let leg = list
        .legs()
        .next()
        .unwrap();
    for warning in leg.warnings() {
        let shown = warning.to_string();
        assert!(!shown.contains("secret"), "{shown}");
    }
    assert!(leg
        .warnings()
        .iter()
        .any(|w| w
            .to_string()
            .contains("nv")));
    match leg.target() {
        LegTarget::Unparsed(unparsed) => {
            let shown = unparsed.to_string();
            assert!(!shown.contains("secret"), "{shown}");
            assert!(
                shown.contains(
                    &"bogus/secret-endpoint"
                        .len()
                        .to_string()
                ),
                "{shown}"
            );
        }
        other => panic!("{other:?}"),
    }
    for warning in [ListWarning::QuoteSpansLegs, ListWarning::CarrierExpands] {
        assert!(!warning
            .to_string()
            .is_empty());
    }
}
