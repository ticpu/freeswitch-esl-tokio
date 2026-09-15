//! Extracts the switch's C that `freeswitch-types` ports from every tree `hooks/source-refs.yaml`
//! names, read out of the clone `FREESWITCH_SOURCE` names, and compiles one unit per tree with its
//! symbols prefixed by the tree's name. Nothing of the FreeSWITCH tree is kept outside `OUT_DIR`.

use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const UTILS: &str = "src/switch_utils.c";

const TOKENIZER: &[&str] = &[
    "unescape_char",
    "cleanup_separated_string",
    "switch_separate_string_string",
    "separate_string_char_delim",
    "separate_string_blank_delim",
    "switch_separate_string",
    "switch_find_end_paren",
];

/// Harness symbols Rust links against, renamed per tree like every extracted function.
const EXPORTS: &[&str] = &["oracle_cleanup", "oracle_char_delim", "oracle_blank_delim"];

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

#[derive(Deserialize)]
struct Index {
    freeswitch: Pinned,
    #[serde(default)]
    trees: Vec<Named>,
}

#[derive(Deserialize)]
struct Pinned {
    commit: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
    commit: String,
    fetch: Option<String>,
}

/// A tree to compile: the pin under the name `pin`, then every entry of `trees`.
struct Tree {
    name: String,
    commit: String,
    public: bool,
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=FREESWITCH_SOURCE");
    println!("cargo::rustc-check-cfg=cfg(c_oracle)");
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let index = manifest.join("../hooks/source-refs.yaml");
    println!("cargo::rerun-if-changed={}", index.display());

    let root = env::var_os("FREESWITCH_SOURCE");
    let root = match root.as_deref() {
        None => Err("FREESWITCH_SOURCE is not set".to_owned()),
        Some(root) if root.is_empty() => Err("FREESWITCH_SOURCE is empty".to_owned()),
        Some(root) => Ok(Path::new(root)),
    };

    let mut table = String::from("static TREES: &[Tree] = &[\n");
    let mut modules = String::new();
    for tree in trees(&index) {
        let abi = match root
            .clone()
            .and_then(|root| Source::open(root, &tree))
        {
            Ok(mut source) => {
                compile(&tree, &mut source, &out);
                println!("cargo::rustc-cfg=c_oracle");
                writeln!(modules, "tree_abi!({0}, \"{0}_\");", tree.name).expect("String write");
                format!("Ok(&{}::ABI)", tree.name)
            }
            Err(missing) => {
                println!(
                    "cargo::warning=C oracle tree {} not built: {missing}",
                    tree.name
                );
                format!("Err({missing:?})")
            }
        };
        writeln!(
            table,
            "    Tree {{ name: {:?}, commit: {:?}, public: {}, abi: {abi} }},",
            tree.name, tree.commit, tree.public
        )
        .expect("String write");
    }
    table.push_str("];\n");
    table.push_str(&modules);
    let generated = out.join("trees.rs");
    fs::write(&generated, table).unwrap_or_else(|e| panic!("writing {}: {e}", generated.display()));
}

fn trees(index: &Path) -> Vec<Tree> {
    let yaml =
        fs::read_to_string(index).unwrap_or_else(|e| panic!("reading {}: {e}", index.display()));
    let index: Index =
        yaml_serde::from_str(&yaml).unwrap_or_else(|e| panic!("parsing {}: {e}", index.display()));
    let pin = Tree {
        name: "pin".to_owned(),
        commit: index
            .freeswitch
            .commit,
        public: true,
    };
    let named = index
        .trees
        .into_iter()
        .map(|tree| {
            let identifier = tree
                .name
                .starts_with(|c: char| c.is_ascii_lowercase())
                && tree
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            assert!(
                identifier && tree.name != "pin",
                "tree name {:?} must be a lowercase identifier other than pin",
                tree.name
            );
            Tree {
                name: tree.name,
                commit: tree.commit,
                public: tree
                    .fetch
                    .is_some(),
            }
        });
    std::iter::once(pin)
        .chain(named)
        .collect()
}

/// The files of one tree, read out of the clone at its commit as they are needed.
struct Source<'r> {
    root: &'r Path,
    commit: String,
    files: HashMap<&'static str, String>,
}

impl<'r> Source<'r> {
    /// The tree, or why its commit cannot be read.
    fn open(root: &'r Path, tree: &Tree) -> Result<Self, String> {
        let output = git(root)
            .arg("cat-file")
            .arg("-e")
            .arg(format!("{}^{{commit}}", tree.commit))
            .output()
            .map_err(|e| format!("running git: {e}"))?;
        if !output
            .status
            .success()
        {
            let stderr = String::from_utf8_lossy(&output.stderr).replace('\n', " ");
            return Err(format!(
                "FREESWITCH_SOURCE has no commit {}: {stderr}",
                tree.commit
            ));
        }
        Ok(Self {
            root,
            commit: tree
                .commit
                .clone(),
            files: HashMap::new(),
        })
    }

    /// `path` at the tree's commit; a commit that has the tree but lacks the file breaks the build.
    fn file(&mut self, path: &'static str) -> &str {
        let (root, commit) = (self.root, &self.commit);
        self.files
            .entry(path)
            .or_insert_with(|| {
                let output = git(root)
                    .arg("show")
                    .arg(format!("{commit}:{path}"))
                    .output()
                    .unwrap_or_else(|e| panic!("running git: {e}"));
                assert!(
                    output
                        .status
                        .success(),
                    "{path} at {commit}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .unwrap_or_else(|e| panic!("{path} at {commit} is not UTF-8: {e}"))
            })
    }

    /// The `#define` line naming `name`; a tree that moved it breaks the build rather than the oracle.
    fn define(&mut self, path: &'static str, name: &str) -> String {
        let commit = self
            .commit
            .clone();
        self.file(path)
            .lines()
            .find(|line| {
                line.strip_prefix("#define ")
                    .and_then(|rest| rest.strip_prefix(name))
                    .is_some_and(|rest| rest.starts_with([' ', '\t']))
            })
            .map(|line| format!("{line}\n"))
            .unwrap_or_else(|| panic!("{path} at {commit} defines no {name}"))
    }

    /// A function's definition, from its signature at column 0 to the brace closing it there.
    fn function(&mut self, path: &'static str, name: &str) -> String {
        let commit = self
            .commit
            .clone();
        let lines: Vec<&str> = self
            .file(path)
            .lines()
            .collect();
        let start = lines
            .iter()
            .position(|line| is_definition(line, name))
            .unwrap_or_else(|| panic!("{path} at {commit} has no definition of {name}"));
        let end = lines[start..]
            .iter()
            .position(|line| *line == "}")
            .map(|at| start + at)
            .unwrap_or_else(|| panic!("{name} in {path} at {commit} never closes"));
        format!("{}\n\n", lines[start..=end].join("\n"))
    }
}

fn git(root: &Path) -> Command {
    let mut git = Command::new("git");
    git.arg("--git-dir")
        .arg(root.join(".git"));
    git
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

fn compile(tree: &Tree, source: &mut Source<'_>, out: &Path) {
    let mut unit = String::from(PRELUDE);
    for symbol in TOKENIZER
        .iter()
        .chain(EXPORTS)
    {
        writeln!(unit, "#define {symbol} {}_{symbol}", tree.name).expect("String write");
    }
    unit.push_str(&source.define(UTILS, "ESCAPE_META"));
    for name in TOKENIZER {
        unit.push_str(&source.function(UTILS, name));
    }
    unit.push_str(WRAPPERS);
    let file = out.join(format!("{}.c", tree.name));
    fs::write(&file, unit).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
    cc::Build::new()
        .file(&file)
        .compile(&format!("freeswitch_{}", tree.name));
}
