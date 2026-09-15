//! `switch_url_encode` and its siblings on every built tree, against the rule that tree follows.
//! Nothing in the workspace ports them; the rules are what a sofia destination meets per tree.

use freeswitch_c_oracle::on_every_tree;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;

/// What `switch_url_encode_opt` does, `double_encode` false, with `%` and two uppercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Percent {
    KeepsEscapes,
    EncodesAlways,
}

/// The buffer `switch_core_url_encode_opt` hands the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoreBuffer {
    FitsTheEncoding,
    InputLength,
}

fn rules(tree: &str) -> (Percent, CoreBuffer) {
    match tree {
        "pin" | "master" => (Percent::KeepsEscapes, CoreBuffer::FitsTheEncoding),
        "fork" => (Percent::EncodesAlways, CoreBuffer::InputLength),
        other => panic!("tree {other} has no url-encode rule; read its switch_url_encode_opt"),
    }
}

const PIECES: &[&[u8]] = &[
    b"%",
    b"%2",
    b"%41",
    b"%4F",
    b"%e9",
    b"%zz",
    b"%%41",
    b" ",
    b"@",
    b":",
    b"a",
    b"Z",
    b"0",
    b"F",
    b"~",
    b"\x7f",
    b"\x01",
    b"\r",
    b"\t",
    b"\xc3\xa9",
    b"\\",
    b"\"",
    b"'",
    b"/",
];

fn url() -> impl Strategy<Value = Vec<u8>> {
    vec(select(PIECES), 0..10).prop_map(|pieces| pieces.concat())
}

fn is_escape(url: &[u8], at: usize) -> bool {
    matches!(url.get(at..at + 3), Some([b'%', high, low]) if [high, low].iter().all(|digit| b"0123456789ABCDEF".contains(digit)))
}

/// The encoding a tree writes into `cap` bytes before the terminator.
fn encoded(unsafe_bytes: &[u8], url: &[u8], cap: usize, keep_escapes: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (at, &byte) in url
        .iter()
        .enumerate()
    {
        if out.len() >= cap {
            break;
        }
        let kept = keep_escapes && is_escape(url, at);
        if !kept && (!(b' '..=b'~').contains(&byte) || unsafe_bytes.contains(&byte)) {
            if out.len() + 3 > cap {
                break;
            }
            out.extend_from_slice(format!("%{byte:02X}").as_bytes());
        } else {
            out.push(byte);
        }
    }
    out
}

#[test]
fn url_encode_follows_the_tree_rule() {
    on_every_tree(
        file!(),
        "url_encode_follows_the_tree_rule",
        (url(), any::<bool>(), select(&[1usize, 2, 3, 4, 8, 64][..])),
        |tree, c, (url, double_encode, len)| {
            let (percent, _) = rules(tree);
            let keep = !double_encode && percent == Percent::KeepsEscapes;
            prop_assert_eq!(
                c.url_encode_opt(&url, len, double_encode),
                encoded(c.url_unsafe(), &url, len - 1, keep),
                "{:?} into {} bytes, double_encode {}",
                String::from_utf8_lossy(&url),
                len,
                double_encode
            );
            prop_assert_eq!(c.url_encode(&url, len), c.url_encode_opt(&url, len, false));
            Ok(())
        },
    );
}

#[test]
fn core_url_encode_sizes_its_buffer_by_tree() {
    on_every_tree(
        file!(),
        "core_url_encode_sizes_its_buffer_by_tree",
        (url(), any::<bool>()),
        |tree, c, (url, double_encode)| {
            let (percent, buffer) = rules(tree);
            let keep = !double_encode && percent == Percent::KeepsEscapes;
            let cap = match buffer {
                CoreBuffer::FitsTheEncoding => usize::MAX,
                CoreBuffer::InputLength => url.len(),
            };
            prop_assert_eq!(
                c.core_url_encode_opt(&url, double_encode),
                encoded(c.url_unsafe(), &url, cap, keep),
                "{:?}, double_encode {}",
                String::from_utf8_lossy(&url),
                double_encode
            );
            Ok(())
        },
    );
}

/// Only `SWITCH_URL_UNSAFE` counts, so a control byte other than CR and LF, DEL or a byte past
/// ASCII needs no encoding; a valid escape is skipped whole.
#[test]
fn needs_url_encode_reads_the_unsafe_set_alone() {
    on_every_tree(
        file!(),
        "needs_url_encode_reads_the_unsafe_set_alone",
        url(),
        |_, c, url| {
            let mut at = 0;
            let mut needs = false;
            while at < url.len() {
                if is_escape(&url, at) {
                    at += 3;
                    continue;
                }
                if c.url_unsafe()
                    .contains(&url[at])
                {
                    needs = true;
                    break;
                }
                at += 1;
            }
            prop_assert_eq!(
                c.needs_url_encode(&url),
                needs,
                "{:?}",
                String::from_utf8_lossy(&url)
            );
            Ok(())
        },
    );
}

#[test]
fn every_tree_shares_one_unsafe_set() {
    let sets: Vec<(&str, &[u8])> = freeswitch_c_oracle::trees()
        .iter()
        .filter_map(|tree| {
            tree.oracle()
                .ok()
                .map(|c| (tree.name(), c.url_unsafe()))
        })
        .collect();
    for (tree, set) in &sets {
        assert_eq!(
            *set, b"\r\n #%&+:;<=>?@[\\]^`{|}\"",
            "SWITCH_URL_UNSAFE on tree {tree}"
        );
    }
}
