//! Pieces of the switch's C cut out of one file's syntax tree, and the directive lines that name them.
//!
//! A piece is the whole lines its tree-sitter node spans; code inside a function is found by a
//! marker line that must occur once in it. A piece that cannot be found is an error.

use tree_sitter::{Node, Parser, Tree};

/// What a `//@` line in an oracle unit takes out of a tree, in the unit's place of that line.
#[derive(Debug, PartialEq, Eq)]
pub enum Directive<'a> {
    /// `//@ define <path> <name>`: the first `#define` of `name`.
    Define { path: &'a str, name: &'a str },
    /// `//@ function <path> <name>`: the definition of `name`.
    Function { path: &'a str, name: &'a str },
    /// `//@ block <path> <function> <marker>`: the statement opening on `marker` in `function`.
    Block {
        path: &'a str,
        function: &'a str,
        marker: &'a str,
    },
    /// `//@ declaration <path> <name>`: the declaration, `typedef` or struct definition of `name`.
    Declaration { path: &'a str, name: &'a str },
    /// `//@ after <path> <function> <anchor> => <marker>`: the statement opening on the first line
    /// reading `marker` after the one line of `function` reading `anchor`.
    After {
        path: &'a str,
        function: &'a str,
        anchor: &'a str,
        marker: &'a str,
    },
    /// `//@ before <path> <function> <anchor> => <marker>`: the statement opening on the last line
    /// reading `marker` before the one line of `function` reading `anchor`.
    Before {
        path: &'a str,
        function: &'a str,
        anchor: &'a str,
        marker: &'a str,
    },
}

/// Which side of its anchor a marker is looked for on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The first marker after the anchor.
    After,
    /// The last marker before the anchor.
    Before,
}

/// One file's text and its C syntax tree.
pub struct Parsed {
    text: String,
    tree: Tree,
}

impl Parsed {
    pub fn new(text: String) -> Result<Self, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|e| format!("loading the C grammar: {e}"))?;
        let tree = parser
            .parse(&text, None)
            .ok_or("tree-sitter returned no tree")?;
        Ok(Self { text, tree })
    }

    /// Every node in document order, inside error nodes too; `bodies` descends into functions.
    fn nodes(&self, bodies: bool) -> Vec<Node<'_>> {
        let mut nodes = Vec::new();
        let mut pending = vec![self
            .tree
            .root_node()];
        while let Some(node) = pending.pop() {
            nodes.push(node);
            if !bodies && node.kind() == "function_definition" {
                continue;
            }
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node
                .children(&mut cursor)
                .collect();
            pending.extend(
                children
                    .into_iter()
                    .rev(),
            );
        }
        nodes
    }

    fn text_of(&self, node: Node<'_>) -> &str {
        &self.text[node.byte_range()]
    }

    /// The whole lines bytes `start..end` touch, newline-terminated.
    fn lines(&self, start: usize, end: usize) -> String {
        let text = &self.text;
        let start = text[..start]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let end = if text[..end].ends_with('\n') {
            end - 1
        } else {
            text[end..]
                .find('\n')
                .map_or(text.len(), |at| end + at)
        };
        format!("{}\n", &text[start..end])
    }

    /// The byte of each line's first non-blank character in `start..end` whose trimmed text is `line`.
    fn reading(&self, start: usize, end: usize, line: &str) -> Vec<usize> {
        let start = self.text[..start]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let mut at = start;
        let mut found = Vec::new();
        for text in self.text[start..end].split_inclusive('\n') {
            if text.trim() == line {
                found.push(
                    at + text.len()
                        - text
                            .trim_start()
                            .len(),
                );
            }
            at += text.len();
        }
        found
    }

    /// The outermost node opening at byte `at`, with a `;` closing it on its heels.
    fn statement_at(&self, at: usize) -> Result<(usize, usize), String> {
        let row = self.text[..at]
            .matches('\n')
            .count()
            + 1;
        let root = self
            .tree
            .root_node();
        let mut node = root
            .named_descendant_for_byte_range(at, at + 1)
            .ok_or_else(|| format!("has no node at line {row}"))?;
        while let Some(parent) = node.parent() {
            if parent.start_byte() != at
                || parent
                    .parent()
                    .is_none()
                || parent.is_error()
            {
                break;
            }
            node = parent;
        }
        if node.start_byte() != at || node.kind() == "comment" {
            return Err(format!("opens no statement at line {row}"));
        }
        if node.has_error() {
            return Err(format!(
                "the {} at line {row} does not parse through line {}",
                node.kind(),
                node.end_position()
                    .row
                    + 1
            ));
        }
        let end = match node.next_sibling() {
            Some(next) if next.kind() == ";" => next.end_byte(),
            _ => node.end_byte(),
        };
        Ok((at, end))
    }

    /// The one definition of the function `name`.
    fn definition(&self, name: &str) -> Result<Node<'_>, String> {
        let found: Vec<Node<'_>> = self
            .nodes(false)
            .into_iter()
            .filter(|node| node.kind() == "function_definition" && self.declares(*node, name))
            .collect();
        match found[..] {
            [node] => Ok(node),
            _ => Err(format!(
                "has {} definitions of {name}, not one",
                found.len()
            )),
        }
    }

    /// Whether a declarator of `node` names `name`.
    fn declares(&self, node: Node<'_>, name: &str) -> bool {
        node.children_by_field_name("declarator", &mut node.walk())
            .any(|declarator| self.declared_name(declarator) == Some(name))
    }

    /// The identifier a declarator names, through pointers, arrays and parentheses.
    fn declared_name(&self, mut node: Node<'_>) -> Option<&str> {
        loop {
            node = match node.kind() {
                "identifier" | "type_identifier" => return Some(self.text_of(node)),
                "parenthesized_declarator" => node.named_child(0)?,
                _ => node.child_by_field_name("declarator")?,
            };
        }
    }
}

impl Directive<'_> {
    /// The file the piece is read from.
    pub fn path(&self) -> &str {
        match self {
            Directive::Define { path, .. }
            | Directive::Function { path, .. }
            | Directive::Block { path, .. }
            | Directive::Declaration { path, .. }
            | Directive::After { path, .. }
            | Directive::Before { path, .. } => path,
        }
    }

    /// The piece cut out of `file`, the file at [`Directive::path`].
    pub fn extract(&self, file: &Parsed) -> Result<String, String> {
        match *self {
            Directive::Define { name, .. } => define(file, name),
            Directive::Function { name, .. } => function(file, name),
            Directive::Block {
                function, marker, ..
            } => block(file, function, marker).map_err(|e| format!("{function}: {e}")),
            Directive::Declaration { name, .. } => declaration(file, name),
            Directive::After {
                function,
                anchor,
                marker,
                ..
            } => beside(file, function, anchor, marker, Side::After)
                .map_err(|e| format!("{function}: {e}")),
            Directive::Before {
                function,
                anchor,
                marker,
                ..
            } => beside(file, function, anchor, marker, Side::Before)
                .map_err(|e| format!("{function}: {e}")),
        }
    }
}

/// The directive `line` holds, `None` for any other line.
pub fn directive(line: &str) -> Result<Option<Directive<'_>>, String> {
    let Some(rest) = line
        .trim_start()
        .strip_prefix("//@")
    else {
        return Ok(None);
    };
    let mut words = rest
        .trim_start()
        .splitn(3, ' ');
    let (Some(kind), Some(path), Some(argument)) = (words.next(), words.next(), words.next())
    else {
        return Err(format!(
            "directive {line:?} names no kind, path and argument"
        ));
    };
    let argument = argument.trim_end();
    let directive = match kind {
        "define" => Directive::Define {
            path,
            name: argument,
        },
        "function" => Directive::Function {
            path,
            name: argument,
        },
        "block" => {
            let Some((function, marker)) = argument.split_once(' ') else {
                return Err(format!("block directive {line:?} names no marker"));
            };
            Directive::Block {
                path,
                function,
                marker,
            }
        }
        "declaration" => Directive::Declaration {
            path,
            name: argument,
        },
        kind @ ("after" | "before") => {
            let Some((function, rest)) = argument.split_once(' ') else {
                return Err(format!("{kind} directive {line:?} names no anchor"));
            };
            let Some((anchor, marker)) = rest.split_once(" => ") else {
                return Err(format!(
                    "{kind} directive {line:?} names no marker after =>"
                ));
            };
            if kind == "after" {
                Directive::After {
                    path,
                    function,
                    anchor,
                    marker,
                }
            } else {
                Directive::Before {
                    path,
                    function,
                    anchor,
                    marker,
                }
            }
        }
        other => return Err(format!("directive {line:?} has unknown kind {other}")),
    };
    Ok(Some(directive))
}

/// The first `#define` naming `name`, continuation lines included.
pub fn define(file: &Parsed, name: &str) -> Result<String, String> {
    let node = file
        .nodes(true)
        .into_iter()
        .find(|node| {
            matches!(node.kind(), "preproc_def" | "preproc_function_def")
                && node
                    .child_by_field_name("name")
                    .is_some_and(|named| file.text_of(named) == name)
        })
        .ok_or_else(|| format!("defines no {name}"))?;
    Ok(file.lines(node.start_byte(), node.end_byte()))
}

/// A function's definition, never its prototype.
pub fn function(file: &Parsed, name: &str) -> Result<String, String> {
    let node = file.definition(name)?;
    Ok(format!(
        "{}\n",
        file.lines(node.start_byte(), node.end_byte())
    ))
}

/// The statement opening on the one line of `function` that reads `marker`, `else` chain included.
pub fn block(file: &Parsed, function: &str, marker: &str) -> Result<String, String> {
    let body = file.definition(function)?;
    let starts = file.reading(body.start_byte(), body.end_byte(), marker);
    let [start] = starts[..] else {
        return Err(format!(
            "reads {marker:?} on {} lines, not one",
            starts.len()
        ));
    };
    let (start, end) = file.statement_at(start)?;
    Ok(file.lines(start, end))
}

/// The statement opening on the nearest line of `function` reading `marker` on `side` of the one
/// line reading `anchor`.
pub fn beside(
    file: &Parsed,
    function: &str,
    anchor: &str,
    marker: &str,
    side: Side,
) -> Result<String, String> {
    let body = file.definition(function)?;
    let (from, to) = (body.start_byte(), body.end_byte());
    let anchors = file.reading(from, to, anchor);
    let [at] = anchors[..] else {
        return Err(format!(
            "reads {anchor:?} on {} lines, not one",
            anchors.len()
        ));
    };
    let markers = file.reading(from, to, marker);
    let start = match side {
        Side::After => markers
            .into_iter()
            .find(|&found| found > at),
        Side::Before => markers
            .into_iter()
            .rev()
            .find(|&found| found < at),
    }
    .ok_or_else(|| format!("reads no {marker:?} beside {anchor:?}"))?;
    let (start, end) = file.statement_at(start)?;
    Ok(file.lines(start, end))
}

/// The one declaration, `typedef` or struct definition naming `name`, through its `;`.
pub fn declaration(file: &Parsed, name: &str) -> Result<String, String> {
    let found: Vec<Node<'_>> = file
        .nodes(true)
        .into_iter()
        .filter(|node| match node.kind() {
            "declaration" | "type_definition" => file.declares(*node, name),
            "struct_specifier" => {
                node.child_by_field_name("body")
                    .is_some()
                    && node
                        .child_by_field_name("name")
                        .is_some_and(|named| file.text_of(named) == name)
            }
            _ => false,
        })
        .collect();
    let [node] = found[..] else {
        return Err(format!("declares {name} {} times, not once", found.len()));
    };
    let (start, end) = file.statement_at(node.start_byte())?;
    Ok(file.lines(start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"#include <switch.h>

#define SHORT 1
#define LONG(a) \
	(a) + \
	1

static int helper(const char *s,
				  int n);

SWITCH_STANDARD_API(helper_function);

static int helper(const char *s,
				  int n)
{
	int x = 0;

	if (s[0] == '{') {
		x = n;
	} else if (s[0] == '}') {
		x = -n;
	}
	else {
		x = 0;
	}

	x += strlen("}");
	return x;
}

SWITCH_STANDARD_API(helper_function)
{
	char *p = strchr(cmd, '/');
	/* { in a comment */
	if (p) {
		*p++ = '\0';
	}

	return SWITCH_STATUS_SUCCESS;
}

struct pair {
	const char *name;
	int value;
};

static struct pair TABLE[] = {
	{"A", 1},
	{NULL, 0}
};

typedef enum {
	FIRST
} other_t;

typedef enum {
	ONE = 1,
	TWO
} number_t;
"#;

    fn parsed(text: &str) -> Parsed {
        Parsed::new(text.to_owned()).expect("C parses")
    }

    #[test]
    fn a_define_takes_its_continuation_lines() {
        let file = parsed(FILE);
        assert_eq!(define(&file, "SHORT"), Ok("#define SHORT 1\n".to_owned()));
        assert_eq!(
            define(&file, "LONG"),
            Ok("#define LONG(a) \\\n\t(a) + \\\n\t1\n".to_owned())
        );
        assert!(define(&file, "SHOR").is_err());
    }

    #[test]
    fn a_function_is_its_definition_not_its_prototype() {
        let file = parsed(FILE);
        let helper = function(&file, "helper").expect("helper is defined");
        assert!(helper.starts_with("static int helper(const char *s,\n\t\t\t\t  int n)\n{"));
        assert!(helper.ends_with("return x;\n}\n\n"));
        let api = function(&file, "helper_function").expect("the API is defined");
        assert!(api.starts_with("SWITCH_STANDARD_API(helper_function)\n{"));
        assert!(function(&file, "help").is_err());
        assert!(function(&file, "missing").is_err());
    }

    #[test]
    fn two_definitions_of_one_name_are_refused() {
        let twice = parsed(&format!(
            "{FILE}\nstatic int helper(void)\n{{\n\treturn 0;\n}}\n"
        ));
        assert_eq!(
            function(&twice, "helper"),
            Err("has 2 definitions of helper, not one".to_owned())
        );
    }

    #[test]
    fn line_drift_moves_nothing_extracted() {
        let file = parsed(FILE);
        let drifted = parsed(&FILE.replace(
            "#include <switch.h>\n",
            "#include <switch.h>\n\n/* inserted */\nstatic int unrelated;\n\n",
        ));
        for name in ["helper", "helper_function"] {
            assert_eq!(function(&file, name), function(&drifted, name));
        }
        let marker = "if (s[0] == '{') {";
        assert_eq!(
            block(&file, "helper", marker),
            block(&drifted, "helper", marker)
        );
    }

    #[test]
    fn a_block_follows_braces_through_else_and_literals() {
        let file = parsed(FILE);
        assert_eq!(
            block(&file, "helper", "if (s[0] == '{') {"),
            Ok("\tif (s[0] == '{') {\n\t\tx = n;\n\t} else if (s[0] == '}') {\n\t\tx = -n;\n\t}\n\telse {\n\t\tx = 0;\n\t}\n".to_owned())
        );
        assert_eq!(
            block(&file, "helper_function", "if (p) {"),
            Ok("\tif (p) {\n\t\t*p++ = '\\0';\n\t}\n".to_owned())
        );
    }

    #[test]
    fn a_block_without_a_brace_is_its_statement() {
        let file = parsed(FILE);
        assert_eq!(
            block(&file, "helper_function", "char *p = strchr(cmd, '/');"),
            Ok("\tchar *p = strchr(cmd, '/');\n".to_owned())
        );
        assert_eq!(
            block(&file, "helper_function", "/* { in a comment */"),
            Err("opens no statement at line 34".to_owned())
        );
    }

    #[test]
    fn a_marker_must_occur_exactly_once() {
        let file = parsed(FILE);
        assert_eq!(
            block(&file, "helper", "x = 0;"),
            Ok("\t\tx = 0;\n".to_owned())
        );
        let twice = parsed(&FILE.replace("x = n;", "x = 0;"));
        assert_eq!(
            block(&twice, "helper", "x = 0;"),
            Err("reads \"x = 0;\" on 2 lines, not one".to_owned())
        );
        assert_eq!(
            block(&file, "helper", "if (t) {"),
            Err("reads \"if (t) {\" on 0 lines, not one".to_owned())
        );
    }

    #[test]
    fn a_marker_beside_its_anchor_is_the_nearest_one() {
        let file = parsed("static int f(void)\n{\n\tx = 1;\n\tif (a) {\n\t\ty();\n\t}\n\tx = 2;\n\tanchor();\n\tx = 1;\n\tz();\n\tx = 1;\n}\n");
        assert_eq!(
            beside(&file, "f", "anchor();", "x = 1;", Side::After),
            Ok("\tx = 1;\n".to_owned())
        );
        assert_eq!(
            beside(&file, "f", "if (a) {", "x = 1;", Side::Before),
            Ok("\tx = 1;\n".to_owned())
        );
        assert_eq!(
            beside(&file, "f", "x = 1;", "z();", Side::After),
            Err("reads \"x = 1;\" on 3 lines, not one".to_owned())
        );
        assert!(beside(&file, "f", "anchor();", "missing();", Side::After).is_err());
        assert!(beside(&file, "f", "if (a) {", "z();", Side::Before).is_err());
    }

    #[test]
    fn beside_directives_name_an_anchor_and_a_marker() {
        assert_eq!(
            directive("//@ after src/a.c f anchor(); => x = 1;"),
            Ok(Some(Directive::After {
                path: "src/a.c",
                function: "f",
                anchor: "anchor();",
                marker: "x = 1;"
            }))
        );
        assert_eq!(
            directive("\t//@ before src/a.c f if (a) { => x = 1;"),
            Ok(Some(Directive::Before {
                path: "src/a.c",
                function: "f",
                anchor: "if (a) {",
                marker: "x = 1;"
            }))
        );
        assert!(directive("//@ before src/a.c f anchor();").is_err());
    }

    #[test]
    fn declarations_and_typedefs_run_to_their_semicolon() {
        let file = parsed(FILE);
        assert_eq!(
            declaration(&file, "pair"),
            Ok("struct pair {\n\tconst char *name;\n\tint value;\n};\n".to_owned())
        );
        assert_eq!(
            declaration(&file, "TABLE"),
            Ok("static struct pair TABLE[] = {\n\t{\"A\", 1},\n\t{NULL, 0}\n};\n".to_owned())
        );
        assert_eq!(
            declaration(&file, "number_t"),
            Ok("typedef enum {\n\tONE = 1,\n\tTWO\n} number_t;\n".to_owned())
        );
        assert!(declaration(&file, "missing_t").is_err());
    }

    #[test]
    fn directives_parse_by_kind() {
        let file = parsed(FILE);
        assert_eq!(directive("\tint x;"), Ok(None));
        assert_eq!(
            directive("//@ function src/a.c helper"),
            Ok(Some(Directive::Function {
                path: "src/a.c",
                name: "helper"
            }))
        );
        assert_eq!(
            directive("\t//@ block src/a.c helper if (s[0] == '{') {"),
            Ok(Some(Directive::Block {
                path: "src/a.c",
                function: "helper",
                marker: "if (s[0] == '{') {"
            }))
        );
        assert_eq!(
            directive("//@ declaration src/a.c TABLE"),
            Ok(Some(Directive::Declaration {
                path: "src/a.c",
                name: "TABLE"
            }))
        );
        assert!(directive("//@ function src/a.c").is_err());
        assert!(directive("//@ fn src/a.c helper").is_err());
        let block = directive("//@ block src/a.c helper_function if (p) {")
            .expect("a block directive")
            .expect("a directive");
        assert_eq!(block.path(), "src/a.c");
        assert_eq!(
            block.extract(&file),
            Ok("\tif (p) {\n\t\t*p++ = '\\0';\n\t}\n".to_owned())
        );
        let elsewhere = directive("//@ block src/a.c helper if (p) {")
            .expect("a block directive")
            .expect("a directive");
        assert_eq!(
            elsewhere.extract(&file),
            Err("helper: reads \"if (p) {\" on 0 lines, not one".to_owned())
        );
        let define = directive("//@ define src/a.c SHORT")
            .expect("a define directive")
            .expect("a directive");
        assert_eq!(define.extract(&file), Ok("#define SHORT 1\n".to_owned()));
        let declaration = directive("//@ declaration src/a.c number_t")
            .expect("a declaration directive")
            .expect("a directive");
        assert!(declaration
            .extract(&file)
            .is_ok());
    }
}
