//! Extracts the switch's C that `freeswitch-types` ports from every tree `hooks/source-refs.yaml`
//! names, read out of the clone `FREESWITCH_SOURCE` names, and compiles one unit per tree with its
//! symbols prefixed by the tree's name. Nothing of the FreeSWITCH tree is kept outside `OUT_DIR`.
//!
//! The unit is the files of `UNITS` joined, each `//@` directive line replaced by what it names in
//! the tree; `extract.rs` holds the directive grammar.

#[path = "build/extract.rs"]
mod extract;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use extract::Directive;

/// The oracle's C, in the order the unit joins it.
const UNITS: &[&str] = &[
    "c/prelude.h",
    "c/strings.c",
    "c/url.c",
    "c/originate.c",
    "c/expand.c",
    "c/api_originate.c",
];

/// A C symbol Rust links against: the `Abi` field holding it and its parameter list and return.
struct Export {
    field: &'static str,
    symbol: &'static str,
    signature: &'static str,
}

const RECORDED: &str = "(input: *const c_char, emit: Emit, ctx: *mut c_void)";

const EXPORTS: &[Export] = &[
    Export {
        field: "cleanup",
        symbol: "oracle_cleanup",
        signature: "(str: *mut c_char, delim: c_char) -> *mut c_char",
    },
    Export {
        field: "char_delim",
        symbol: "oracle_char_delim",
        signature: "(buf: *mut c_char, delim: c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint",
    },
    Export {
        field: "blank_delim",
        symbol: "oracle_blank_delim",
        signature: "(buf: *mut c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint",
    },
    Export {
        field: "separate_string",
        symbol: "switch_separate_string",
        signature: "(buf: *mut c_char, delim: c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint",
    },
    Export {
        field: "separate_string_string",
        symbol: "switch_separate_string_string",
        signature: "(buf: *mut c_char, delim: *mut c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint",
    },
    Export {
        field: "find_end_paren",
        symbol: "switch_find_end_paren",
        signature: "(s: *const c_char, open: c_char, close: c_char) -> *mut c_char",
    },
    Export {
        field: "url_encode_opt",
        symbol: "switch_url_encode_opt",
        signature: "(url: *const c_char, buf: *mut c_char, len: usize, double_encode: c_int) -> *mut c_char",
    },
    Export {
        field: "url_encode",
        symbol: "switch_url_encode",
        signature: "(url: *const c_char, buf: *mut c_char, len: usize) -> *mut c_char",
    },
    Export {
        field: "needs_url_encode",
        symbol: "oracle_needs_url_encode",
        signature: "(s: *const c_char) -> c_int",
    },
    Export {
        field: "core_url_encode_opt",
        symbol: "oracle_core_url_encode_opt",
        signature: "(url: *const c_char, double_encode: c_int, out: *mut c_char, outlen: usize) -> usize",
    },
    Export {
        field: "url_unsafe",
        symbol: "oracle_url_unsafe",
        signature: "() -> *const c_char",
    },
    Export {
        field: "brackets",
        symbol: "oracle_brackets",
        signature: "(data: *mut c_char, a: c_char, b: c_char, c: c_char, emit: Emit, ctx: *mut c_void) -> c_long",
    },
    Export {
        field: "dial",
        symbol: "oracle_dial",
        signature: RECORDED,
    },
    Export {
        field: "expand",
        symbol: "oracle_expand",
        signature: RECORDED,
    },
    Export {
        field: "api_originate",
        symbol: "oracle_api_originate",
        signature: RECORDED,
    },
    Export {
        field: "switch_true",
        symbol: "oracle_switch_true",
        signature: "(expr: *const c_char) -> c_int",
    },
];

/// What the harness reports through its callback, numbered alike in C and in Rust.
const TAGS: &[&str] = &[
    "PAIR",
    "THREAD",
    "GROUP",
    "LEG",
    "ENDPOINT",
    "NESTED",
    "FAILURE",
    "LOOKUP",
    "API",
    "ORIGINATE",
    "CALLER_ID",
    "APPLICATION",
    "TRANSFER",
    "CONTEXT",
    "OUTPUT",
];

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
    println!("cargo::rerun-if-changed=build");
    println!("cargo::rerun-if-changed=c");
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
    let units = read_units(&manifest);

    let mut generated = abi();
    generated.push_str("static TREES: &[Tree] = &[\n");
    let mut modules = String::new();
    for tree in trees(&index) {
        let abi = match root
            .clone()
            .and_then(|root| Source::open(root, &tree))
        {
            Ok(mut source) => {
                compile(&tree, &mut source, &units, &out);
                println!("cargo::rustc-cfg=c_oracle");
                modules.push_str(&tree_module(&tree.name));
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
            generated,
            "    Tree {{ name: {:?}, commit: {:?}, public: {}, abi: {abi} }},",
            tree.name, tree.commit, tree.public
        )
        .expect("String write");
    }
    generated.push_str("];\n");
    generated.push_str(&modules);
    let file = out.join("trees.rs");
    fs::write(&file, generated).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
}

/// Every unit's text, in `UNITS` order.
fn read_units(manifest: &Path) -> Vec<(&'static str, String)> {
    UNITS
        .iter()
        .map(|&unit| {
            let path = manifest.join(unit);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            (unit, text)
        })
        .collect()
}

/// The tag constants and the `Abi` struct every tree module fills.
fn abi() -> String {
    let mut generated = String::new();
    for (number, tag) in TAGS
        .iter()
        .enumerate()
    {
        writeln!(generated, "const {tag}: c_int = {};", number + 1).expect("String write");
    }
    generated.push_str("#[derive(Debug)]\nstruct Abi {\n");
    for export in EXPORTS {
        writeln!(
            generated,
            "    {}: unsafe extern \"C\" fn{},",
            export.field, export.signature
        )
        .expect("String write");
    }
    generated.push_str("}\n");
    generated
}

/// The module linking one tree's prefixed symbols into its `ABI`.
fn tree_module(tree: &str) -> String {
    let mut module = format!("mod {tree} {{\n    use super::*;\n    extern \"C\" {{\n");
    for export in EXPORTS {
        writeln!(
            module,
            "        #[link_name = \"{tree}_{}\"]\n        fn {}{};",
            export.symbol, export.field, export.signature
        )
        .expect("String write");
    }
    module.push_str("    }\n    pub(super) static ABI: Abi = Abi {\n");
    for export in EXPORTS {
        writeln!(module, "        {},", export.field).expect("String write");
    }
    module.push_str("    };\n}\n");
    module
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
    files: HashMap<String, String>,
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
    fn file(&mut self, path: &str) -> &str {
        let (root, commit) = (self.root, &self.commit);
        self.files
            .entry(path.to_owned())
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

    /// What `directive` names in this tree; a tree that lacks it breaks the build.
    fn extract(&mut self, directive: &Directive<'_>) -> String {
        let commit = self
            .commit
            .clone();
        let path = directive.path();
        directive
            .extract(self.file(path))
            .unwrap_or_else(|e| panic!("{path} at {commit}: {e}"))
    }
}

fn git(root: &Path) -> Command {
    let mut git = Command::new("git");
    git.arg("--git-dir")
        .arg(root.join(".git"));
    git
}

fn compile(tree: &Tree, source: &mut Source<'_>, units: &[(&str, String)], out: &Path) {
    let mut body = String::new();
    let mut symbols: Vec<&str> = Vec::new();
    let mut taken = HashSet::new();
    for (unit, text) in units {
        for line in text.lines() {
            let directive = extract::directive(line).unwrap_or_else(|e| panic!("{unit}: {e}"));
            let Some(directive) = directive else {
                body.push_str(line);
                body.push('\n');
                continue;
            };
            if let Directive::Function { name, .. } = directive {
                assert!(taken.insert(name), "{unit}: function {name} is taken twice");
                symbols.push(name);
            }
            body.push_str(&source.extract(&directive));
        }
    }
    let mut unit = String::new();
    for (number, tag) in TAGS
        .iter()
        .enumerate()
    {
        writeln!(unit, "#define ORACLE_{tag} {}", number + 1).expect("String write");
    }
    let exported = EXPORTS
        .iter()
        .map(|export| export.symbol)
        .filter(|symbol| !taken.contains(symbol));
    for symbol in symbols
        .into_iter()
        .chain(exported)
    {
        writeln!(unit, "#define {symbol} {}_{symbol}", tree.name).expect("String write");
    }
    unit.push_str(&body);
    let file = out.join(format!("{}.c", tree.name));
    fs::write(&file, unit).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
    // The unit is the switch's code as each tree ships it, so its warnings are not ours to fix.
    cc::Build::new()
        .warnings(false)
        .file(&file)
        .compile(&format!("freeswitch_{}", tree.name));
}
