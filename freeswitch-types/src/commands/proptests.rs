//! Every render read back through the port of the switch's own passes, over text weighted
//! toward what those passes treat specially. A render either arrives as built, or config
//! load or parse refuses what was built.

use std::str::FromStr;

use freeswitch_c_oracle::{against_the_c, config, Oracle};
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;

use super::{
    originate_quote, originate_unquote, AudioEndpoint, BlockParse, DialString, DialStringCarrier,
    DialStringTarget, Endpoint, ErrorEndpoint, FlattenedDialString, FlattenedLeg, GroupCall,
    GroupCallOrder, LegTarget, LoopbackEndpoint, Originate, SofiaContact, SofiaEndpoint,
    SofiaGateway, UserEndpoint, Variables, VariablesType,
};
use crate::channel::HangupCause;
use crate::switch_passes::brackets::PairEffect;
use crate::switch_passes::expansion::names_a_variable;
use crate::switch_passes::originate_function::c_oracle::{
    build_originate, intended, originate_spec, strip_whitespace, switch_view,
};
use crate::switch_passes::pipeline;
use crate::switch_passes::separate::separate;
use crate::switch_passes::{trace, untrace};
use crate::test_text::text;
use crate::variables::VariableName;

fn scope() -> impl Strategy<Value = VariablesType> {
    prop_oneof![
        Just(VariablesType::Enterprise),
        Just(VariablesType::Default),
        Just(VariablesType::Channel),
    ]
}

/// Any ASCII byte and a non-ASCII char; `build_vars` drops the separators `with_separator` refuses.
fn block_separator() -> impl Strategy<Value = Option<char>> {
    prop_oneof![
        2 => Just(None),
        1 => prop_oneof![
            4 => (0u8..0x80).prop_map(char::from),
            1 => select(&['é', '😀'][..]),
        ]
        .prop_map(Some),
    ]
}

fn block_parses() -> Vec<BlockParse> {
    match BlockParse::default() {
        BlockParse::PairSplitCleans => vec![BlockParse::PairSplitCleans],
    }
}

fn api_targets() -> Vec<DialStringTarget> {
    let api = DialStringTarget::new(DialStringCarrier::EslApi);
    let separated = ['~', '!'].map(|sep| {
        api.with_argv_separator(sep)
            .expect("a usable argv separator")
    });
    let mut targets = vec![api];
    targets.extend(separated);
    targets
}

fn targets() -> Vec<DialStringTarget> {
    let mut carriers = api_targets();
    carriers.push(DialStringTarget::new(DialStringCarrier::Dialplan));
    block_parses()
        .into_iter()
        .flat_map(|block_parse| {
            carriers
                .iter()
                .map(move |target| target.with_block_parse(block_parse))
        })
        .collect()
}

/// A key the switch reads as text, or `None` for one named by its position.
type Entry = (Option<String>, String);

fn entries(count: std::ops::Range<usize>) -> impl Strategy<Value = Vec<Entry>> {
    vec((option::weighted(0.3, text()), text()), count)
}

fn plain(values: &[String]) -> Vec<Entry> {
    values
        .iter()
        .map(|value| (None, value.clone()))
        .collect()
}

fn build_vars(
    scope: VariablesType,
    prefix: &str,
    entries: &[Entry],
    sep: Option<char>,
) -> Option<Variables> {
    let vars = Variables::with_vars(
        scope,
        entries
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                (
                    key.clone()
                        .unwrap_or_else(|| format!("{prefix}{i}")),
                    value.clone(),
                )
            }),
    );
    match sep {
        Some(sep) => vars
            .with_separator(sep)
            .ok(),
        None => Some(vars),
    }
}

fn pairs(vars: &Variables) -> Vec<(String, String)> {
    vars.iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn config_refuses_vars(vars: &Variables) -> bool {
    let json = serde_json::to_value(vars).expect("Variables serializes");
    serde_json::from_value::<Variables>(json).is_err()
}

/// Left to the switch, as the `Variables` rustdoc and dial-string-format.md's *A value naming a
/// variable* say, so no escaping delivers it verbatim.
fn left_to_the_switch(vars: &Variables) -> bool {
    vars.iter()
        .any(|(key, value)| names_a_variable(key) || names_a_variable(value))
}

/// The one leg a single-endpoint dial string reads to: every pair installed, and its endpoint.
fn read_single_leg(
    dial: &str,
    target: DialStringTarget,
) -> Result<(Vec<(String, String)>, String), String> {
    let list = pipeline::read(dial, target).map_err(|e| format!("{e:?}"))?;
    let [thread] = &list.threads[..] else {
        return Err(format!(
            "{} threads",
            list.threads
                .len()
        ));
    };
    let [group] = &thread.groups[..] else {
        return Err(format!(
            "{} groups",
            thread
                .groups
                .len()
        ));
    };
    let [leg] = &group[..] else {
        return Err(format!("{} legs", group.len()));
    };
    let installed = list
        .blocks
        .iter()
        .chain(&thread.blocks)
        .chain(&leg.blocks)
        .flat_map(|block| &block.pairs)
        .map(|pair| match &pair.effect {
            PairEffect::Set(value) => Ok((
                pair.key
                    .clone(),
                value.clone(),
            )),
            effect => Err(format!("{} {effect:?}", pair.key)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        installed,
        leg.endpoint
            .clone(),
    ))
}

fn bare_endpoint() -> impl Strategy<Value = Endpoint> {
    prop_oneof![
        (text(), option::of(text()), option::of(text())).prop_map(
            |(extension, context, dialplan)| {
                let mut ep = LoopbackEndpoint::new(extension);
                ep.context = context;
                ep.dialplan = dialplan;
                ep.into()
            }
        ),
        (text(), text())
            .prop_map(|(profile, destination)| { SofiaEndpoint::new(profile, destination).into() }),
        (text(), text(), option::of(text())).prop_map(|(gateway, destination, profile)| {
            let mut ep = SofiaGateway::new(gateway, destination);
            ep.profile = profile;
            ep.into()
        }),
        (text(), option::of(text())).prop_map(|(name, domain)| {
            let mut ep = UserEndpoint::new(name);
            ep.domain = domain;
            ep.into()
        }),
        select(
            &[
                HangupCause::UserBusy,
                HangupCause::NoRouteDestination,
                HangupCause::NormalClearing,
            ][..]
        )
        .prop_map(|cause| ErrorEndpoint::new(cause).into()),
        (text(), text(), option::of(text())).prop_map(|(user, domain, profile)| {
            let mut ep = SofiaContact::new(user, domain);
            ep.profile = profile;
            ep.into()
        }),
        (
            text(),
            text(),
            option::of(select(
                &[
                    GroupCallOrder::All,
                    GroupCallOrder::Enterprise,
                    GroupCallOrder::First,
                ][..]
            ))
        )
            .prop_map(|(group, domain, order)| {
                let mut ep = GroupCall::new(group, domain);
                ep.order = order;
                ep.into()
            }),
        (
            select(&[Endpoint::PortAudio, Endpoint::PulseAudio, Endpoint::Alsa][..]),
            option::of(text())
        )
            .prop_map(|(variant, destination)| {
                let mut ep = AudioEndpoint::new();
                ep.destination = destination;
                variant(ep)
            }),
    ]
}

/// Left to the switch like a block value naming a variable; an expression's own `${` is not.
fn fields_name_a_variable(bare: &Endpoint) -> bool {
    match bare {
        Endpoint::SofiaContact(ep) => [
            Some(&ep.user),
            Some(&ep.domain),
            ep.profile
                .as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|field| names_a_variable(field)),
        Endpoint::GroupCall(ep) => names_a_variable(&ep.group) || names_a_variable(&ep.domain),
        other => names_a_variable(&other.module_text()),
    }
}

/// A loopback dialplan without a context renders the switch's default context in the gap.
fn canonical(endpoint: &Endpoint) -> Endpoint {
    let mut endpoint = endpoint.clone();
    if let Endpoint::Loopback(loopback) = &mut endpoint {
        if loopback
            .dialplan
            .is_some()
            && loopback
                .context
                .is_none()
        {
            loopback.context = Some("default".to_owned());
        }
    }
    endpoint
}

#[derive(Debug, Clone)]
struct LegSpec {
    values: Vec<String>,
    endpoint: &'static str,
}

#[derive(Debug, Clone)]
struct ListSpec {
    enterprise: Option<Vec<String>>,
    default: Option<Vec<String>>,
    legs: Vec<LegSpec>,
    separators: Vec<&'static str>,
    keep: Vec<bool>,
}

const LEG_ENDPOINTS: &[&str] = &["loopback/9199/test", "error/USER_BUSY", "null/a"];

fn list_spec() -> impl Strategy<Value = ListSpec> {
    let leg = (vec(text(), 0..3), select(LEG_ENDPOINTS))
        .prop_map(|(values, endpoint)| LegSpec { values, endpoint });
    (
        option::of(vec(text(), 1..3)),
        option::of(vec(text(), 1..3)),
        vec(leg, 1..5),
        vec(select(&[",", "|"][..]), 4),
        vec(any::<bool>(), 5),
    )
        .prop_map(|(enterprise, default, legs, separators, keep)| ListSpec {
            enterprise,
            default,
            legs,
            separators,
            keep,
        })
}

/// A leg as a caller reads it: what it dials, and each key named in the spec with its value.
type LegView = (String, Vec<(String, Option<String>)>);

struct RenderedList {
    text: String,
    legs: Vec<LegView>,
}

/// The list the spec describes, every block rendered inside the argument and the whole list
/// escaped as one, or `None` when a value is one config load refuses or the switch expands.
fn render_list(spec: &ListSpec, target: DialStringTarget) -> Option<RenderedList> {
    let inner = target.inner();
    let mut text = String::new();
    let mut inherited = Vec::new();
    let block = |scope, prefix: &str, values: &[String], text: &mut String| {
        let vars = build_vars(scope, prefix, &plain(values), None)?;
        if vars.is_empty() {
            return Some(Vec::new());
        }
        if config_refuses_vars(&vars) || left_to_the_switch(&vars) {
            return None;
        }
        text.push_str(
            &vars
                .display_for(inner)
                .to_string(),
        );
        Some(pairs(&vars))
    };
    for (scope, prefix, values) in [
        (VariablesType::Enterprise, "e", &spec.enterprise),
        (VariablesType::Default, "g", &spec.default),
    ] {
        if let Some(values) = values {
            inherited.extend(block(scope, prefix, values, &mut text)?);
        }
    }
    let mut legs = Vec::new();
    for (i, leg) in spec
        .legs
        .iter()
        .enumerate()
    {
        if i > 0 {
            text.push_str(spec.separators[i - 1]);
        }
        let own = block(
            VariablesType::Channel,
            &format!("l{i}_"),
            &leg.values,
            &mut text,
        )?;
        text.push_str(leg.endpoint);
        let vars = own
            .into_iter()
            .chain(
                inherited
                    .iter()
                    .cloned(),
            )
            .map(|(key, value)| (key, Some(value)))
            .collect();
        legs.push((
            leg.endpoint
                .to_owned(),
            vars,
        ));
    }
    let text = match target.escape_argument(&text) {
        Some(escaped) => escaped.into_owned(),
        None => text,
    };
    Some(RenderedList { text, legs })
}

fn endpoint_text(target: &LegTarget) -> String {
    match target {
        LegTarget::Error(error) => format!("error/{}", error.as_written()),
        LegTarget::Endpoint(endpoint) => endpoint.to_string(),
        LegTarget::Unparsed(unparsed) => unparsed
            .endpoint()
            .to_owned(),
    }
}

struct Key<'a>(&'a str);

impl VariableName for Key<'_> {
    fn as_str(&self) -> &str {
        self.0
    }
}

fn view(leg: &FlattenedLeg, keys: &LegView) -> LegView {
    (
        endpoint_text(leg.target()),
        keys.1
            .iter()
            .map(|(key, _)| {
                (
                    key.clone(),
                    leg.variable(Key(key))
                        .map(str::to_owned),
                )
            })
            .collect(),
    )
}

fn views(list: &FlattenedDialString, keys: &[LegView]) -> Vec<LegView> {
    list.legs()
        .zip(keys)
        .map(|(leg, keys)| view(leg, keys))
        .collect()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn variables_arrive_as_built_or_are_refused(
        scope in scope(),
        sep in block_separator(),
        values in entries(1..4),
    ) {
        let Some(vars) = build_vars(scope, "v", &values, sep) else {
            return Ok(());
        };
        if left_to_the_switch(&vars) {
            return Ok(());
        }
        let refused = config_refuses_vars(&vars);
        let want = pairs(&vars);
        for target in targets() {
            let block = vars.display_for(target).to_string();
            let dial = format!("{block}null/drift");
            let got = read_single_leg(&dial, target);
            let delivered = got.as_ref().is_ok_and(|(installed, endpoint)| {
                *installed == want && endpoint == "null/drift"
            });
            prop_assert!(
                delivered || refused,
                "{scope:?} ^^{sep:?} at {target:?}: rendered {dial:?}, port read {got:?}, want {want:?}"
            );
            if delivered && !refused {
                let parsed = Variables::parse_for(&block, target).map(|vars| pairs(&vars));
                prop_assert_eq!(parsed, Ok(want.clone()), "parse of {:?} at {:?}", block, target);
            }
        }
    }

    #[test]
    fn endpoints_arrive_as_built_or_are_refused(
        bare in bare_endpoint(),
        vars in option::of((scope(), block_separator(), entries(1..3))),
    ) {
        if fields_name_a_variable(&bare) {
            return Ok(());
        }
        let mut endpoint = bare.clone();
        if let Some((scope, sep, values)) = &vars {
            let Some(vars) = build_vars(*scope, "v", values, *sep) else {
                return Ok(());
            };
            if left_to_the_switch(&vars) {
                return Ok(());
            }
            endpoint.set_variables(Some(vars));
        }
        let want = endpoint
            .variables()
            .map(pairs)
            .unwrap_or_default();
        let refused = serde_json::to_value(&endpoint)
            .and_then(serde_json::from_value::<Endpoint>)
            .is_err();
        for target in targets() {
            let rendered = endpoint.display_for(target).to_string();
            let got = read_single_leg(&rendered, target);
            let delivered = got.as_ref().is_ok_and(|(installed, text)| {
                *installed == want
                    && *text == bare.module_text()
                    && Endpoint::parse_bare(text).as_ref() == Ok(&canonical(&bare))
            });
            let parsed = Endpoint::parse_for(&rendered, target);
            prop_assert!(
                delivered || refused,
                "{endpoint:?} at {target:?}: rendered {rendered:?}, port read {got:?}"
            );
            prop_assert!(
                parsed.is_ok() || refused,
                "{endpoint:?} at {target:?}: rendered {rendered:?} loads from config, parse {parsed:?}"
            );
            if delivered && !refused {
                let parsed = parsed.map(|mut parsed| {
                    let vars = parsed.variables().map(|vars| (vars.scope(), pairs(vars)));
                    parsed.set_variables(None);
                    (parsed, vars)
                });
                let built = endpoint.variables().map(|vars| (vars.scope(), pairs(vars)));
                prop_assert_eq!(parsed, Ok((canonical(&bare), built)), "parse of {:?} at {:?}", rendered, target);
            }
        }
    }

    #[test]
    fn originate_positionals_arrive_as_built_or_are_refused(spec in originate_spec()) {
        let Some(originate) = build_originate(&spec) else {
            return Ok(());
        };
        let rendered = originate.to_string();
        let want = intended(&spec, &originate);
        let got = switch_view(&rendered);
        let delivered = got.as_ref() == Ok(&want);
        let parsed = Originate::from_str(&rendered);
        let refused = serde_json::to_value(&originate)
            .and_then(serde_json::from_value::<Originate>)
            .is_err()
            || parsed.is_err();
        prop_assert!(
            delivered || refused,
            "{spec:?}: rendered {rendered:?}, switch reads {got:?}, want {want:?}"
        );
        if delivered {
            let rerendered = parsed.map(|parsed| parsed.to_string());
            prop_assert_eq!(
                rerendered.as_deref().map(switch_view),
                Ok(Ok(want)),
                "parse of {:?} renders {:?}", rendered, rerendered
            );
        }
    }

    #[test]
    fn an_escaped_argument_is_one_argument(text in text()) {
        prop_assert_eq!(
            DialStringTarget::new(DialStringCarrier::Dialplan).escape_argument(&text),
            None
        );
        for target in api_targets() {
            let escaped = target
                .escape_argument(&text)
                .expect("an API target escapes");
            let (prefix, sep) = match target.argv_separator() {
                Some(sep) => (format!("^^{sep}"), sep),
                None => (String::new(), ' '),
            };
            let x = "x".to_owned();
            let y = "y".to_owned();
            for (line, want) in [
                (format!("{prefix}{escaped}{sep}y"), vec![text.clone(), y.clone()]),
                (format!("{prefix}x{sep}{escaped}{sep}y"), vec![x.clone(), text.clone(), y.clone()]),
                (format!("{prefix}x{sep}{escaped}"), vec![x.clone(), text.clone()]),
            ] {
                let split = separate(&trace(strip_whitespace(&line)), ' ', usize::MAX);
                let tokens: Vec<String> = split
                    .tokens
                    .iter()
                    .map(|token| untrace(&token.text))
                    .collect();
                prop_assert_eq!(tokens, want, "{:?} at {:?}", line, target);
                prop_assert!(!split.open_quote, "{line:?}");
            }
        }
    }

    #[test]
    fn a_flattened_list_reads_back_after_retain_and_rerender(spec in list_spec()) {
        for target in targets() {
            let Some(rendered) = render_list(&spec, target) else {
                return Ok(());
            };
            let mut list = FlattenedDialString::parse_for(&rendered.text, target)
                .map_err(|e| TestCaseError::fail(format!("{:?} at {target:?}: {e:?}", rendered.text)))?;
            prop_assert_eq!(list.display_raw().to_string(), rendered.text.clone());
            prop_assert_eq!(views(&list, &rendered.legs), rendered.legs.clone(), "{:?} at {:?}", rendered.text, target);
            prop_assert!(list.warnings().is_empty(), "{:?}: {:?}", rendered.text, list.warnings());

            let canonical = list.display_for(target).to_string();
            let reread = FlattenedDialString::parse_for(&canonical, target)
                .map_err(|e| TestCaseError::fail(format!("{canonical:?} at {target:?}: {e:?}")))?;
            prop_assert_eq!(views(&reread, &rendered.legs), rendered.legs.clone(), "display_for {:?} at {:?}", canonical, target);

            let raws: Vec<String> = list.legs().map(|leg| leg.raw().to_owned()).collect();
            let mut at = 0;
            list.retain(|_| {
                let keep = spec.keep[at];
                at += 1;
                keep
            });
            let kept: Vec<usize> = (0..spec.legs.len()).filter(|&i| spec.keep[i]).collect();
            if kept.is_empty() {
                prop_assert!(list.is_empty());
                continue;
            }
            let forwarded = list.display_raw().to_string();
            let back = FlattenedDialString::parse_for(&forwarded, target)
                .map_err(|e| TestCaseError::fail(format!("{forwarded:?} at {target:?}: {e:?}")))?;
            let want: Vec<LegView> = kept.iter().map(|&i| rendered.legs[i].clone()).collect();
            let want_raws: Vec<String> = kept.iter().map(|&i| raws[i].clone()).collect();
            prop_assert_eq!(back.legs().map(|leg| leg.raw().to_owned()).collect::<Vec<_>>(), want_raws, "{:?}", forwarded);
            prop_assert_eq!(views(&back, &want), want, "retained {:?} at {:?}", forwarded, target);
        }
    }

    #[test]
    fn a_quoted_token_is_one_argument_of_the_blank_split(value in text()) {
        let quoted = originate_quote(&value);
        prop_assert_eq!(originate_unquote(&quoted), value.clone());
        let line = format!("x {quoted} y");
        let split = separate(&trace(&line), ' ', usize::MAX);
        let tokens: Vec<String> = split
            .tokens
            .iter()
            .map(|token| untrace(&token.text))
            .collect();
        prop_assert_eq!(tokens, vec!["x".to_owned(), value, "y".to_owned()], "{:?}", line);
        prop_assert!(!split.open_quote, "{line:?}");
    }
}

/// The pairs and endpoint text the switch's C reads of `dial` at `target`: `originate_function`
/// on the API line or the dialplan carrier's expansion, then `switch_ivr_originate`'s passes.
fn c_reads_the_leg(
    c: Oracle,
    dial: &str,
    target: DialStringTarget,
) -> Result<(Vec<(String, String)>, String), String> {
    let utf8 = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string());
    let aleg = match target.carrier() {
        DialStringCarrier::Dialplan => {
            c.expand(dial.as_bytes())
                .text
        }
        DialStringCarrier::EslApi => c
            .api_originate(originate_line(dial, target).as_bytes())
            .originated
            .and_then(|originated| originated.aleg)
            .ok_or("originate_function dials nothing")?,
    };
    let read = c.dial(&aleg);
    let [thread] = &read.threads[..] else {
        return Err(format!(
            "{:?} in {} threads",
            read.failure,
            read.threads
                .len()
        ));
    };
    if let Some(failure) = &thread.failure {
        return Err(utf8(failure)?);
    }
    let [group] = &thread.groups[..] else {
        return Err(format!(
            "{} groups",
            thread
                .groups
                .len()
        ));
    };
    let [leg] = &group[..] else {
        return Err(format!("{} legs", group.len()));
    };
    let installed = read
        .enterprise
        .iter()
        .chain(&thread.pairs)
        .chain(&leg.pairs)
        .map(|(key, value)| Ok((utf8(key)?, utf8(value)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let endpoint = leg
        .endpoint
        .as_deref()
        .ok_or("the leg has no endpoint")?;
    Ok((installed, utf8(endpoint)?))
}

/// The line `originate` reads for `dial` at `target`, the dial string its first argument.
fn originate_line(dial: &str, target: DialStringTarget) -> String {
    match target.argv_separator() {
        Some(argv) => format!("^^{argv}{dial}{argv}&park()"),
        None => format!("{dial} &park()"),
    }
}

/// A block of every scope at every target, read by the switch's C rather than the port.
#[test]
fn variables_arrive_through_the_c_passes() {
    against_the_c(
        file!(),
        "variables_arrive_through_the_c_passes",
        (scope(), block_separator(), entries(1..4)),
        |c, (scope, sep, values)| {
            let Some(vars) = build_vars(scope, "v", &values, sep) else {
                return Ok(());
            };
            if left_to_the_switch(&vars) || config_refuses_vars(&vars) {
                return Ok(());
            }
            let want = Ok((pairs(&vars), "null/drift".to_owned()));
            for target in targets() {
                let dial = format!("{}null/drift", vars.display_for(target));
                prop_assert_eq!(
                    &c_reads_the_leg(c, &dial, target),
                    &want,
                    "{:?} at {:?}",
                    dial,
                    target
                );
            }
            Ok(())
        },
    );
}

/// Endpoints under a block of any scope or none at every target, read by the switch's C.
#[test]
fn endpoints_arrive_through_the_c_passes() {
    against_the_c(
        file!(),
        "endpoints_arrive_through_the_c_passes",
        (
            bare_endpoint(),
            option::of((scope(), block_separator(), entries(1..3))),
        ),
        |c, (bare, vars)| {
            if fields_name_a_variable(&bare) {
                return Ok(());
            }
            let mut endpoint = bare.clone();
            if let Some((scope, sep, values)) = vars {
                let Some(vars) = build_vars(scope, "v", &values, sep) else {
                    return Ok(());
                };
                if left_to_the_switch(&vars) || config_refuses_vars(&vars) {
                    return Ok(());
                }
                endpoint.set_variables(Some(vars));
            }
            let refused = serde_json::to_value(&endpoint)
                .and_then(serde_json::from_value::<Endpoint>)
                .is_err();
            if refused {
                return Ok(());
            }
            let installed = endpoint
                .variables()
                .map(pairs)
                .unwrap_or_default();
            let want = Ok((installed, bare.module_text()));
            let expression = matches!(bare, Endpoint::SofiaContact(_) | Endpoint::GroupCall(_));
            for target in targets() {
                // The expansion substitutes the expression, which the stub answers with nothing.
                if expression && target.carrier() == DialStringCarrier::Dialplan {
                    continue;
                }
                let dial = endpoint
                    .display_for(target)
                    .to_string();
                prop_assert_eq!(
                    &c_reads_the_leg(c, &dial, target),
                    &want,
                    "{:?} at {:?}",
                    dial,
                    target
                );
            }
            Ok(())
        },
    );
}
