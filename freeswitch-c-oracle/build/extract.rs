//! Pieces of the switch's C cut out of one file's text, and the directive lines that name them.
//!
//! A function is found by its definition at column 0 whatever line it sits on; code inside a
//! function by a marker line that must occur once in it. A piece that cannot be found is an error.

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
    /// `//@ declaration <path> <opening>`: the declaration opening on the column-0 line `opening`.
    Declaration { path: &'a str, opening: &'a str },
    /// `//@ typedef <path> <name>`: the `typedef` closing on `} name;`.
    Typedef { path: &'a str, name: &'a str },
}

impl Directive<'_> {
    /// The file the piece is read from.
    pub fn path(&self) -> &str {
        match self {
            Directive::Define { path, .. }
            | Directive::Function { path, .. }
            | Directive::Block { path, .. }
            | Directive::Declaration { path, .. }
            | Directive::Typedef { path, .. } => path,
        }
    }

    /// The piece cut out of `text`, the file at [`Directive::path`].
    pub fn extract(&self, text: &str) -> Result<String, String> {
        match *self {
            Directive::Define { name, .. } => define(text, name),
            Directive::Function { name, .. } => function(text, name),
            Directive::Block {
                function: name,
                marker,
                ..
            } => block(&function(text, name)?, marker).map_err(|e| format!("{name}: {e}")),
            Directive::Declaration { opening, .. } => declaration(text, opening),
            Directive::Typedef { name, .. } => typedef(text, name),
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
            opening: argument,
        },
        "typedef" => Directive::Typedef {
            path,
            name: argument,
        },
        other => return Err(format!("directive {line:?} has unknown kind {other}")),
    };
    Ok(Some(directive))
}

/// The first `#define` naming `name`, continuation lines included.
pub fn define(text: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .collect();
    let start = lines
        .iter()
        .position(|line| {
            line.strip_prefix("#define ")
                .and_then(|rest| rest.strip_prefix(name))
                .is_some_and(|rest| rest.starts_with([' ', '\t', '(']))
        })
        .ok_or_else(|| format!("defines no {name}"))?;
    let end = lines[start..]
        .iter()
        .position(|line| !line.ends_with('\\'))
        .map_or(lines.len() - 1, |at| start + at);
    Ok(format!("{}\n", lines[start..=end].join("\n")))
}

/// A function's definition, from its signature at column 0 to the brace closing it there.
pub fn function(text: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&at| is_definition(&lines[at..], name))
        .collect();
    let [start] = starts[..] else {
        return Err(format!(
            "has {} definitions of {name}, not one",
            starts.len()
        ));
    };
    let end = lines[start..]
        .iter()
        .position(|line| *line == "}")
        .map(|at| start + at)
        .ok_or_else(|| format!("{name} never closes"))?;
    Ok(format!("{}\n\n", lines[start..=end].join("\n")))
}

/// The statement opening on the one line of `body` that reads `marker`: through the brace
/// balancing its first and any `else` chained on, or through its `;` when it opens no brace.
pub fn block(body: &str, marker: &str) -> Result<String, String> {
    let lines: Vec<&str> = body
        .lines()
        .collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == marker)
        .map(|(at, _)| at)
        .collect();
    let [start] = starts[..] else {
        return Err(format!(
            "reads {marker:?} on {} lines, not one",
            starts.len()
        ));
    };
    let end = statement_end(&lines, start).ok_or_else(|| format!("{marker:?} never closes"))?;
    Ok(format!("{}\n", lines[start..=end].join("\n")))
}

/// The declaration opening on the one column-0 line reading `opening`, through its `;`.
pub fn declaration(text: &str, opening: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end() == opening)
        .map(|(at, _)| at)
        .collect();
    let [start] = starts[..] else {
        return Err(format!(
            "opens {opening:?} on {} lines, not one",
            starts.len()
        ));
    };
    let end = statement_end(&lines, start).ok_or_else(|| format!("{opening:?} never closes"))?;
    Ok(format!("{}\n", lines[start..=end].join("\n")))
}

/// The `typedef` closing on the one column-0 line `} name;`, from the `typedef` opening it.
pub fn typedef(text: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .collect();
    let closing = format!("}} {name};");
    let ends: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end() == closing)
        .map(|(at, _)| at)
        .collect();
    let [end] = ends[..] else {
        return Err(format!(
            "closes {closing:?} on {} lines, not one",
            ends.len()
        ));
    };
    let start = lines[..end]
        .iter()
        .rposition(|line| line.starts_with("typedef "))
        .ok_or_else(|| format!("opens no typedef before {closing:?}"))?;
    match statement_end(&lines, start) {
        Some(at) if at == end => Ok(format!("{}\n", lines[start..=end].join("\n"))),
        _ => Err(format!(
            "the typedef before {closing:?} does not close there"
        )),
    }
}

/// The line a statement opening on `lines[start]` ends on.
fn statement_end(lines: &[&str], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut closed = false;
    let mut lexer = Lexer::default();
    for (at, line) in lines
        .iter()
        .enumerate()
        .skip(start)
    {
        let mut after_close = String::new();
        for c in line.chars() {
            let Some(c) = lexer.code(c) else {
                continue;
            };
            match c {
                ';' if depth == 0 && !closed => return Some(at),
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        closed = true;
                        after_close.clear();
                        continue;
                    }
                }
                _ => {}
            }
            after_close.push(c);
        }
        lexer.end_line();
        if !closed || depth > 0 {
            continue;
        }
        let trailing = after_close.trim();
        if trailing.starts_with("else") {
            continue;
        }
        if !trailing.is_empty() {
            return Some(at);
        }
        let next = lines[at + 1..]
            .iter()
            .find(|line| {
                !line
                    .trim()
                    .is_empty()
            });
        if !next.is_some_and(|line| {
            line.trim_start()
                .starts_with("else")
        }) {
            return Some(at);
        }
    }
    None
}

/// `lines[0]` opens the definition of `name`: its signature at column 0, whose parameter list
/// closes before a `{`, where a declaration closes before a `;`.
fn is_definition(lines: &[&str], name: &str) -> bool {
    let line = lines[0];
    if line.starts_with([' ', '\t', '#', '}', '/', '*']) {
        return false;
    }
    let named = line
        .match_indices(name)
        .any(|(at, _)| {
            line[at + name.len()..].starts_with(['(', ')']) && line[..at].ends_with([' ', '*', '('])
        });
    if !named {
        return false;
    }
    let mut depth = 0usize;
    let mut opened = false;
    let mut lexer = Lexer::default();
    for line in lines {
        for c in line.chars() {
            let Some(c) = lexer.code(c) else {
                continue;
            };
            match c {
                '(' => {
                    depth += 1;
                    opened = true;
                }
                ')' => depth = depth.saturating_sub(1),
                c if c.is_whitespace() || c.is_ascii_alphanumeric() || c == '_' || c == '*' => {}
                c if opened && depth == 0 => return c == '{',
                _ => {}
            }
        }
        lexer.end_line();
    }
    false
}

/// Just enough of C's lexical grammar to tell a brace in code from one in a literal or comment.
#[derive(Default)]
struct Lexer {
    state: Lexed,
    previous: Option<char>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Lexed {
    #[default]
    Code,
    Char,
    Str,
    LineComment,
    BlockComment,
}

impl Lexer {
    /// `c` when it is code, `None` when it sits in a literal or comment.
    fn code(&mut self, c: char) -> Option<char> {
        let previous = self
            .previous
            .replace(c);
        let escaped = previous == Some('\\');
        match self.state {
            Lexed::Code => match c {
                '\'' => self.state = Lexed::Char,
                '"' => self.state = Lexed::Str,
                '/' if previous == Some('/') => self.state = Lexed::LineComment,
                '*' if previous == Some('/') => self.state = Lexed::BlockComment,
                c => return Some(c),
            },
            Lexed::Char if c == '\'' && !escaped => self.state = Lexed::Code,
            Lexed::Str if c == '"' && !escaped => self.state = Lexed::Code,
            Lexed::BlockComment if c == '/' && previous == Some('*') => self.state = Lexed::Code,
            _ => {}
        }
        if escaped && c == '\\' {
            self.previous = None;
        }
        None
    }

    fn end_line(&mut self) {
        if self.state == Lexed::LineComment {
            self.state = Lexed::Code;
        }
        self.previous = None;
    }
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

    #[test]
    fn a_define_takes_its_continuation_lines() {
        assert_eq!(define(FILE, "SHORT"), Ok("#define SHORT 1\n".to_owned()));
        assert_eq!(
            define(FILE, "LONG"),
            Ok("#define LONG(a) \\\n\t(a) + \\\n\t1\n".to_owned())
        );
        assert!(define(FILE, "SHOR").is_err());
    }

    #[test]
    fn a_function_is_its_definition_not_its_prototype() {
        let helper = function(FILE, "helper").expect("helper is defined");
        assert!(helper.starts_with("static int helper(const char *s,\n\t\t\t\t  int n)\n{"));
        assert!(helper.ends_with("return x;\n}\n\n"));
        let api = function(FILE, "helper_function").expect("the API is defined");
        assert!(api.starts_with("SWITCH_STANDARD_API(helper_function)\n{"));
        assert!(function(FILE, "help").is_err());
        assert!(function(FILE, "missing").is_err());
    }

    #[test]
    fn two_definitions_of_one_name_are_refused() {
        let twice = format!("{FILE}\nstatic int helper(void)\n{{\n\treturn 0;\n}}\n");
        assert_eq!(
            function(&twice, "helper"),
            Err("has 2 definitions of helper, not one".to_owned())
        );
    }

    #[test]
    fn line_drift_moves_nothing_extracted() {
        let drifted = FILE.replace(
            "#include <switch.h>\n",
            "#include <switch.h>\n\n/* inserted */\nstatic int unrelated;\n\n",
        );
        for name in ["helper", "helper_function"] {
            assert_eq!(function(FILE, name), function(&drifted, name));
        }
        let marker = "if (s[0] == '{') {";
        let body = function(FILE, "helper").expect("helper is defined");
        let drifted_body = function(&drifted, "helper").expect("helper is defined");
        assert_eq!(block(&body, marker), block(&drifted_body, marker));
    }

    #[test]
    fn a_block_follows_braces_through_else_and_literals() {
        let body = function(FILE, "helper").expect("helper is defined");
        assert_eq!(
            block(&body, "if (s[0] == '{') {"),
            Ok("\tif (s[0] == '{') {\n\t\tx = n;\n\t} else if (s[0] == '}') {\n\t\tx = -n;\n\t}\n\telse {\n\t\tx = 0;\n\t}\n".to_owned())
        );
        let api = function(FILE, "helper_function").expect("the API is defined");
        assert_eq!(
            block(&api, "if (p) {"),
            Ok("\tif (p) {\n\t\t*p++ = '\\0';\n\t}\n".to_owned())
        );
    }

    #[test]
    fn a_block_without_a_brace_is_its_statement() {
        let api = function(FILE, "helper_function").expect("the API is defined");
        assert_eq!(
            block(&api, "char *p = strchr(cmd, '/');"),
            Ok("\tchar *p = strchr(cmd, '/');\n".to_owned())
        );
    }

    #[test]
    fn a_marker_must_occur_exactly_once() {
        let body = function(FILE, "helper").expect("helper is defined");
        assert_eq!(block(&body, "x = 0;"), Ok("\t\tx = 0;\n".to_owned()));
        let twice = body.replace("x = n;", "x = 0;");
        assert_eq!(
            block(&twice, "x = 0;"),
            Err("reads \"x = 0;\" on 2 lines, not one".to_owned())
        );
        assert_eq!(
            block(&body, "if (t) {"),
            Err("reads \"if (t) {\" on 0 lines, not one".to_owned())
        );
    }

    #[test]
    fn declarations_and_typedefs_run_to_their_semicolon() {
        assert_eq!(
            declaration(FILE, "struct pair {"),
            Ok("struct pair {\n\tconst char *name;\n\tint value;\n};\n".to_owned())
        );
        assert_eq!(
            declaration(FILE, "static struct pair TABLE[] = {"),
            Ok("static struct pair TABLE[] = {\n\t{\"A\", 1},\n\t{NULL, 0}\n};\n".to_owned())
        );
        assert_eq!(
            typedef(FILE, "number_t"),
            Ok("typedef enum {\n\tONE = 1,\n\tTWO\n} number_t;\n".to_owned())
        );
        assert!(typedef(FILE, "missing_t").is_err());
        assert!(declaration(FILE, "struct missing {").is_err());
    }

    #[test]
    fn directives_parse_by_kind() {
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
            directive("//@ declaration src/a.c static struct pair TABLE[] = {"),
            Ok(Some(Directive::Declaration {
                path: "src/a.c",
                opening: "static struct pair TABLE[] = {"
            }))
        );
        assert!(directive("//@ function src/a.c").is_err());
        assert!(directive("//@ fn src/a.c helper").is_err());
        let block = directive("//@ block src/a.c helper_function if (p) {")
            .expect("a block directive")
            .expect("a directive");
        assert_eq!(block.path(), "src/a.c");
        assert_eq!(
            block.extract(FILE),
            Ok("\tif (p) {\n\t\t*p++ = '\\0';\n\t}\n".to_owned())
        );
        let elsewhere = directive("//@ block src/a.c helper if (p) {")
            .expect("a block directive")
            .expect("a directive");
        assert_eq!(
            elsewhere.extract(FILE),
            Err("helper: reads \"if (p) {\" on 0 lines, not one".to_owned())
        );
        let define = directive("//@ define src/a.c SHORT")
            .expect("a define directive")
            .expect("a directive");
        assert_eq!(define.extract(FILE), Ok("#define SHORT 1\n".to_owned()));
        let typedef = directive("//@ typedef src/a.c number_t")
            .expect("a typedef directive")
            .expect("a directive");
        assert!(typedef
            .extract(FILE)
            .is_ok());
    }
}
