//! Extracts the switch's C that `freeswitch-types` ports from every tree `hooks/source-refs.yaml`
//! names, read out of the clone `FREESWITCH_SOURCE` names, and compiles one unit per tree with its
//! symbols prefixed by the tree's name. Nothing of the FreeSWITCH tree is kept outside `OUT_DIR`.
//!
//! The unit is the files of `UNITS` joined, each `//@` directive line replaced by what it names in
//! the tree; `extract.rs` holds the directive grammar.

#[path = "build/extract.rs"]
mod extract;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use extract::{Directive, Parsed};

/// The oracle's C, in the order the unit joins it.
const UNITS: &[&str] = &[
    "c/prelude.h",
    "c/strings.c",
    "c/url.c",
    "c/originate.c",
    "c/expand.c",
    "c/api_originate.c",
    "c/inline_dialplan.c",
    "c/cause.c",
    "c/sofia.h",
    "c/protect_dest_uri.c",
    "c/sofia_outgoing.c",
    "c/sofia_contact.c",
    "c/loopback.c",
    "c/user.c",
    "c/group_call.c",
];

/// The domain the core's default-domain stub answers with.
const DEFAULT_DOMAIN: &str = "default.example.com";

/// A C symbol Rust links against: the `Abi` field holding it and its parameter list and return.
struct Export<'a> {
    field: &'a str,
    symbol: &'a str,
    signature: &'a str,
}

/// `//@ export <symbol> <signature>`, its field the symbol without its `oracle_` or `switch_`.
fn export(line: &str) -> Option<Export<'_>> {
    let (symbol, signature) = line
        .trim_start()
        .strip_prefix("//@ export ")?
        .split_once(' ')?;
    let field = symbol
        .strip_prefix("oracle_")
        .or_else(|| symbol.strip_prefix("switch_"))
        .unwrap_or_else(|| panic!("export {symbol} is named neither oracle_ nor switch_"));
    Some(Export {
        field,
        symbol,
        signature,
    })
}

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
    "HEADER",
    "VARIABLE",
    "DOMAIN",
    "EXTENSION",
    "CAUSE",
    "RESULT",
    "FIELD",
    "PROFILE",
    "GATEWAY",
    "REGISTRATION",
    "HOST",
    "SELECT",
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
    let exports: Vec<Export<'_>> = units
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter_map(export)
        .collect();

    let mut generated = abi(&exports);
    generated += "static TREES: &[Tree] = &[\n";
    let mut modules = String::new();
    for tree in trees(&index) {
        let abi = match root
            .clone()
            .and_then(|root| Source::open(root, &tree))
        {
            Ok(mut source) => {
                compile(&mut source, &units, &exports, &out);
                println!("cargo::rustc-cfg=c_oracle");
                modules += &tree_module(&tree.name, &exports);
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
        generated += &format!(
            "    Tree {{ name: {:?}, commit: {:?}, public: {}, abi: {abi} }},\n",
            tree.name, tree.commit, tree.public
        );
    }
    generated += "];\n";
    generated += &modules;
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
fn abi(exports: &[Export<'_>]) -> String {
    let mut generated = String::new();
    for (number, tag) in (1..).zip(TAGS) {
        generated += &format!("const {tag}: c_int = {number};\n");
    }
    generated += &format!("/// The domain every tree's core answers as its default.\npub const DEFAULT_DOMAIN: &[u8] = b{DEFAULT_DOMAIN:?};\n");
    generated += "#[derive(Debug)]\nstruct Abi {\n";
    for export in exports {
        generated += &format!(
            "    {}: unsafe extern \"C\" fn{},\n",
            export.field, export.signature
        );
    }
    generated + "}\n"
}

/// The module linking one tree's prefixed symbols into its `ABI`.
fn tree_module(tree: &str, exports: &[Export<'_>]) -> String {
    let mut module = format!("mod {tree} {{\n    use super::*;\n    extern \"C\" {{\n");
    for export in exports {
        module += &format!(
            "        #[link_name = \"{tree}_{}\"]\n        fn {}{};\n",
            export.symbol, export.field, export.signature
        );
    }
    module += "    }\n    pub(super) static ABI: Abi = Abi {\n";
    for export in exports {
        module += &format!("        {},\n", export.field);
    }
    module + "    };\n}\n"
}

fn trees(index: &Path) -> Vec<Tree> {
    let yaml =
        fs::read_to_string(index).unwrap_or_else(|e| panic!("reading {}: {e}", index.display()));
    let index: Index =
        yaml_serde::from_str(&yaml).unwrap_or_else(|e| panic!("parsing {}: {e}", index.display()));
    let mut trees = vec![Tree {
        name: "pin".to_owned(),
        commit: index
            .freeswitch
            .commit,
        public: true,
    }];
    for Named {
        name,
        commit,
        fetch,
    } in index.trees
    {
        let identifier = name.starts_with(|c: char| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        assert!(
            identifier && name != "pin",
            "tree name {name:?} must be a lowercase identifier other than pin"
        );
        trees.push(Tree {
            name,
            commit,
            public: fetch.is_some(),
        });
    }
    trees
}

/// The files of one tree, read out of the clone at its commit and parsed as they are needed.
struct Source<'r> {
    root: &'r Path,
    tree: &'r Tree,
    files: HashMap<String, Parsed>,
}

impl<'r> Source<'r> {
    /// The tree, or why its commit cannot be read.
    fn open(root: &'r Path, tree: &'r Tree) -> Result<Self, String> {
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
            tree,
            files: HashMap::new(),
        })
    }

    /// `path` at the tree's commit; a commit that has the tree but lacks the file breaks the build.
    fn file(&mut self, path: &str) -> &Parsed {
        let (root, commit) = (
            self.root,
            &self
                .tree
                .commit,
        );
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
                let text = String::from_utf8(output.stdout)
                    .unwrap_or_else(|e| panic!("{path} at {commit} is not UTF-8: {e}"));
                Parsed::new(text).unwrap_or_else(|e| panic!("{path} at {commit}: {e}"))
            })
    }

    /// What `line`, holding `directive`, names in this tree; a tree that lacks it breaks the build.
    fn extract(&mut self, line: &str, directive: &Directive<'_>) -> String {
        let label = format!(
            "tree {} ({})",
            self.tree
                .name,
            self.tree
                .commit
        );
        let path = directive.path;
        directive
            .extract(self.file(path))
            .unwrap_or_else(|e| panic!("{label}, {path}, {:?}: {e}", line.trim()))
    }
}

fn git(root: &Path) -> Command {
    let mut git = Command::new("git");
    git.arg("--git-dir")
        .arg(root.join(".git"));
    git
}

fn compile(source: &mut Source<'_>, units: &[(&str, String)], exports: &[Export<'_>], out: &Path) {
    let tree = source.tree;
    let mut body = String::new();
    let mut symbols: Vec<&str> = Vec::new();
    for (unit, text) in units {
        for line in text.lines() {
            if export(line).is_some() {
                continue;
            }
            let directive = extract::directive(line).unwrap_or_else(|e| panic!("{unit}: {e}"));
            let Some(directive) = directive else {
                body += &format!("{line}\n");
                continue;
            };
            if directive.kind == "function" {
                let name = directive.argument;
                assert!(
                    !symbols.contains(&name),
                    "{unit}: function {name} is taken twice"
                );
                symbols.push(name);
            }
            body += &source.extract(line, &directive);
        }
    }
    let mut unit = String::new();
    for (number, tag) in (1..).zip(TAGS) {
        unit += &format!("#define ORACLE_{tag} {number}\n");
    }
    unit += &format!("#define ORACLE_DEFAULT_DOMAIN {DEFAULT_DOMAIN:?}\n");
    for symbol in &symbols {
        unit += &format!("#define {symbol} {}_{symbol}\n", tree.name);
    }
    for export in exports {
        if !symbols.contains(&export.symbol) {
            unit += &format!("#define {0} {1}_{0}\n", export.symbol, tree.name);
        }
    }
    unit += &body;
    let file = out.join(format!("{}.c", tree.name));
    fs::write(&file, unit).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
    // The unit is the switch's code as each tree ships it, so its warnings are not ours to fix.
    cc::Build::new()
        .warnings(false)
        .file(&file)
        .compile(&format!("freeswitch_{}", tree.name));
}
