//! Extracts the switch's string tokenizer from `src/switch_utils.c` at the commit pinned in
//! `hooks/source-refs.yaml`, read out of the clone `FREESWITCH_SOURCE` names, and compiles it.
//! Nothing of the FreeSWITCH tree is kept outside `OUT_DIR`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FUNCTIONS: &[&str] = &[
    "unescape_char",
    "cleanup_separated_string",
    "switch_separate_string_string",
    "separate_string_char_delim",
    "separate_string_blank_delim",
    "switch_separate_string",
    "switch_find_end_paren",
];

const PRELUDE: &str = "#include <string.h>\n#define SWITCH_DECLARE(type) type\n";

const WRAPPERS: &str = r"
char *oracle_cleanup(char *str, char delim)
{
	return cleanup_separated_string(str, delim);
}

unsigned int oracle_char_delim(char *buf, char delim, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_char_delim(buf, delim, array, arraylen);
}

unsigned int oracle_blank_delim(char *buf, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_blank_delim(buf, array, arraylen);
}
";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=FREESWITCH_SOURCE");
    println!("cargo::rustc-check-cfg=cfg(c_oracle)");
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let index = manifest.join("../hooks/source-refs.yaml");
    println!("cargo::rerun-if-changed={}", index.display());
    match pinned_source(&index) {
        Ok((pin, source)) => compile(&pin, &source),
        Err(missing) => {
            println!("cargo::warning=C oracle not built: {missing}");
            println!("cargo::rustc-env=C_ORACLE_MISSING={missing}");
        }
    }
}

/// The pinned commit and `src/switch_utils.c` at it, or why neither can be read.
fn pinned_source(index: &Path) -> Result<(String, String), String> {
    let root = env::var_os("FREESWITCH_SOURCE").ok_or("FREESWITCH_SOURCE is not set")?;
    if root.is_empty() {
        return Err("FREESWITCH_SOURCE is empty".to_owned());
    }
    let yaml =
        fs::read_to_string(index).map_err(|e| format!("reading {}: {e}", index.display()))?;
    let pin = yaml
        .lines()
        .find_map(|line| {
            line.trim_start()
                .strip_prefix("commit:")
        })
        .map(|sha| {
            sha.trim()
                .to_owned()
        })
        .ok_or_else(|| format!("{} names no commit", index.display()))?;
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(Path::new(&root).join(".git"))
        .arg("show")
        .arg(format!("{pin}:src/switch_utils.c"))
        .output()
        .map_err(|e| format!("running git: {e}"))?;
    if !output
        .status
        .success()
    {
        let stderr = String::from_utf8_lossy(&output.stderr).replace('\n', " ");
        return Err(format!(
            "FREESWITCH_SOURCE has no src/switch_utils.c at {pin}: {stderr}"
        ));
    }
    let source = String::from_utf8(output.stdout)
        .map_err(|e| format!("src/switch_utils.c at {pin} is not UTF-8: {e}"))?;
    Ok((pin, source))
}

fn compile(pin: &str, source: &str) {
    let mut unit = String::from(PRELUDE);
    unit.push_str(define(pin, source, "ESCAPE_META"));
    unit.push('\n');
    for name in FUNCTIONS {
        unit.push_str(&function(pin, source, name));
    }
    unit.push_str(WRAPPERS);
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("switch_tokenizer.c");
    fs::write(&out, unit).unwrap_or_else(|e| panic!("writing {}: {e}", out.display()));
    cc::Build::new()
        .file(&out)
        .compile("switch_tokenizer");
    println!("cargo::rustc-cfg=c_oracle");
}

/// The `#define` line naming `name`; a pin that moved it breaks the build rather than the oracle.
fn define<'s>(pin: &str, source: &'s str, name: &str) -> &'s str {
    source
        .lines()
        .find(|line| {
            line.strip_prefix("#define ")
                .and_then(|rest| rest.strip_prefix(name))
                .is_some_and(|rest| rest.starts_with([' ', '\t']))
        })
        .unwrap_or_else(|| panic!("switch_utils.c at {pin} defines no {name}"))
}

/// A function's definition, from its signature at column 0 to the brace closing it there.
fn function(pin: &str, source: &str, name: &str) -> String {
    let lines: Vec<&str> = source
        .lines()
        .collect();
    let start = lines
        .iter()
        .position(|line| is_definition(line, name))
        .unwrap_or_else(|| panic!("switch_utils.c at {pin} has no definition of {name}"));
    let end = lines[start..]
        .iter()
        .position(|line| *line == "}")
        .map(|at| start + at)
        .unwrap_or_else(|| panic!("{name} in switch_utils.c at {pin} never closes"));
    format!("{}\n\n", lines[start..=end].join("\n"))
}

fn is_definition(line: &str, name: &str) -> bool {
    !line.starts_with([' ', '\t', '#'])
        && !line.ends_with(';')
        && line
            .match_indices(name)
            .any(|(at, _)| {
                line[at + name.len()..].starts_with('(') && line[..at].ends_with([' ', '*'])
            })
}
