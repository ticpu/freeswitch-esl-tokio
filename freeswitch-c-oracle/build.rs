//! Extracts the switch's C that `freeswitch-types` ports from every tree `hooks/source-refs.yaml`
//! names, read out of the clone `FREESWITCH_SOURCE` names, and compiles one unit per tree with its
//! symbols prefixed by the tree's name. Nothing of the FreeSWITCH tree is kept outside `OUT_DIR`.
//!
//! Functions are taken whole by name. Code living inside a larger function is taken as the
//! brace-balanced statement opening on a marker line, which must occur once in that function.

use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const UTILS: &str = "src/switch_utils.c";
const UTILS_H: &str = "src/include/switch_utils.h";
const TYPES_H: &str = "src/include/switch_types.h";
const EVENT: &str = "src/switch_event.c";
const CHANNEL: &str = "src/switch_channel.c";
const ORIGINATE: &str = "src/switch_ivr_originate.c";
const COMMANDS: &str = "src/mod/applications/mod_commands/mod_commands.c";

/// `(file, name)` of every `#define` the unit takes, in dependency order.
const DEFINES: &[(&str, &str)] = &[
    (UTILS, "ESCAPE_META"),
    (UTILS_H, "end_of_p"),
    (UTILS_H, "SWITCH_URL_UNSAFE"),
    (UTILS_H, "switch_goto_status"),
    (UTILS_H, "switch_safe_free"),
    (TYPES_H, "SWITCH_ENT_ORIGINATE_DELIM"),
    (TYPES_H, "SWITCH_BLANK_STRING"),
    (TYPES_H, "SWITCH_STANDARD_API"),
    (CHANNEL, "resize"),
    (ORIGINATE, "QUOTED_ESC_COMMA"),
    (ORIGINATE, "UNQUOTED_ESC_COMMA"),
    (ORIGINATE, "MAX_PEERS"),
    (COMMANDS, "ORIGINATE_SYNTAX"),
];

/// `(file, name)` of every function the unit takes whole, in dependency order.
const FUNCTIONS: &[(&str, &str)] = &[
    (UTILS_H, "_zstr"),
    (UTILS_H, "switch_toupper"),
    (UTILS_H, "switch_strchr_strict"),
    (UTILS_H, "switch_string_has_escaped_data"),
    (UTILS_H, "switch_string_var_check_const"),
    (UTILS_H, "switch_needs_url_encode"),
    (UTILS, "unescape_char"),
    (UTILS, "cleanup_separated_string"),
    (UTILS, "switch_separate_string_string"),
    (UTILS, "separate_string_char_delim"),
    (UTILS, "separate_string_blank_delim"),
    (UTILS, "switch_separate_string"),
    (UTILS, "switch_find_end_paren"),
    (UTILS, "switch_url_encode_opt"),
    (UTILS, "switch_url_encode"),
    (UTILS, "switch_core_url_encode_opt"),
    (UTILS, "switch_stristr"),
    (UTILS, "switch_strip_whitespace"),
    (UTILS, "switch_is_number"),
    (UTILS_H, "switch_true"),
    (EVENT, "switch_event_create_brackets"),
    (CHANNEL, "switch_channel_expand_variables_check"),
    (COMMANDS, "originate_function"),
];

/// `(file, function, marker, placeholder)`: the statement a harness function names by placeholder.
const BLOCKS: &[(&str, &str, &str, &str)] = &[
    (
        ORIGINATE,
        "switch_ivr_originate",
        "if (*data == '<') {",
        "ORACLE_ORIGINATE_ULTRA_GLOBAL",
    ),
    (
        ORIGINATE,
        "switch_ivr_originate",
        "while (*data == '{') {",
        "ORACLE_ORIGINATE_GLOBAL",
    ),
    (
        ORIGINATE,
        "switch_ivr_originate",
        "while (p && *p) {",
        "ORACLE_ORIGINATE_COMMA_SCAN",
    ),
    (
        ORIGINATE,
        "switch_ivr_originate",
        "while (*chan_type == '[') {",
        "ORACLE_ORIGINATE_LOCAL",
    ),
    (
        ORIGINATE,
        "switch_ivr_enterprise_originate",
        "while (data && *data == '<') {",
        "ORACLE_ENTERPRISE_ULTRA_GLOBAL",
    ),
];

/// Harness symbols Rust links against, renamed per tree like every extracted function.
const EXPORTS: &[&str] = &[
    "oracle_cleanup",
    "oracle_char_delim",
    "oracle_blank_delim",
    "oracle_needs_url_encode",
    "oracle_core_url_encode_opt",
    "oracle_url_unsafe",
    "oracle_brackets",
    "oracle_dial",
    "oracle_expand",
    "oracle_api_originate",
    "oracle_switch_true",
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

/// The switch's types, reduced to what the extracted code touches.
const PRELUDE: &str = r#"#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#define SWITCH_DECLARE(type) type
#define _In_opt_z_
#define _In_opt_
#define _In_
#define _Check_return_

/* A backslash ending a string steps over its terminator; a second NUL stops that read inside the
   copy, as every oracle buffer does, where the switch would read past its allocation. */
static char *oracle_strdup(const char *s)
{
	size_t len = strlen(s);
	char *copy = calloc(len + 2, 1);

	if (copy) {
		memcpy(copy, s, len);
	}
	return copy;
}
#undef strdup
#define strdup(s) oracle_strdup(s)

#define zstr(x) _zstr(x)
static void oracle_assert_failed(const char *expr);
#define switch_assert(expr) do { if (!(expr)) { oracle_assert_failed(#expr); } } while (0)
#define switch_log_printf(...) ((void) 0)
typedef int switch_bool_t;
#define SWITCH_FALSE 0
#define SWITCH_TRUE 1
typedef size_t switch_size_t;
typedef enum {
	SWITCH_STATUS_SUCCESS,
	SWITCH_STATUS_FALSE,
	SWITCH_STATUS_GENERR
} switch_status_t;

typedef struct switch_memory_pool {
	void *allocs[4];
	int count;
} switch_memory_pool_t;

static void *oracle_pool_alloc(switch_memory_pool_t *pool, size_t size)
{
	if (pool->count == sizeof(pool->allocs) / sizeof(pool->allocs[0])) {
		abort();
	}
	return pool->allocs[pool->count++] = calloc(1, size);
}

static char *oracle_pool_strdup(switch_memory_pool_t *pool, const char *s)
{
	return strcpy(oracle_pool_alloc(pool, strlen(s) + 1), s);
}

#define switch_core_alloc(_pool, _mem) oracle_pool_alloc(_pool, _mem)
#define switch_core_strdup(_pool, _todup) oracle_pool_strdup(_pool, _todup)

typedef void (*oracle_emit_fn)(void *ctx, int tag, const char *a, const char *b);
static _Thread_local oracle_emit_fn oracle_emit;
static _Thread_local void *oracle_ctx;

static void oracle_record(int tag, const char *a, const char *b)
{
	oracle_emit(oracle_ctx, tag, a, b);
}

/* A harness that arms the jump reports a failed assertion and returns; any other aborts. */
static _Thread_local jmp_buf *oracle_assert_jump;

static void oracle_assert_failed(const char *expr)
{
	if (!oracle_assert_jump) {
		abort();
	}
	oracle_record(ORACLE_FAILURE, expr, NULL);
	longjmp(*oracle_assert_jump, 1);
}

/* Every header an extracted pass installs is reported; nothing is stored. */
typedef struct switch_event {
	int flags;
} switch_event_t;
#define EF_UNIQ_HEADERS 1
#define SWITCH_EVENT_CHANNEL_DATA 0
#define SWITCH_STACK_BOTTOM 0

static switch_status_t switch_event_create_plain(switch_event_t **event, int id)
{
	(void) id;
	*event = calloc(1, sizeof(**event));
	return *event ? SWITCH_STATUS_SUCCESS : SWITCH_STATUS_FALSE;
}

static void switch_event_destroy(switch_event_t **event)
{
	free(*event);
	*event = NULL;
}

static switch_status_t switch_event_add_header_string(switch_event_t *event, int stack, const char *name, const char *value)
{
	(void) event;
	(void) stack;
	oracle_record(ORACLE_PAIR, name, value);
	return SWITCH_STATUS_SUCCESS;
}

static int switch_event_check_permission_list(switch_event_t *list, const char *name)
{
	(void) list;
	(void) name;
	return 1;
}

typedef struct switch_core_session {
	int unused;
} switch_core_session_t;
typedef struct switch_channel {
	switch_core_session_t *session;
} switch_channel_t;
typedef struct switch_caller_extension {
	int unused;
} switch_caller_extension_t;
typedef int switch_call_cause_t;
#define SWITCH_CAUSE_NORMAL_CLEARING 16
#define SOF_NONE 0
#define SCF_API_EXPANSION 0

typedef struct switch_stream_handle switch_stream_handle_t;
struct switch_stream_handle {
	switch_status_t (*write_function)(switch_stream_handle_t *handle, const char *fmt, ...);
	void *data;
};
#define SWITCH_STANDARD_STREAM(s) memset(&s, 0, sizeof(s)); s.data = malloc(1)

/* Variable and API lookups answer nothing, so a reference expands to an empty string. */
static const char *switch_channel_get_variable_dup(switch_channel_t *channel, const char *varname, switch_bool_t dup, int idx)
{
	(void) channel;
	(void) dup;
	(void) idx;
	oracle_record(ORACLE_LOOKUP, varname, NULL);
	return NULL;
}

static int switch_core_test_flag(int flag)
{
	(void) flag;
	return 1;
}

static switch_status_t switch_api_execute(const char *cmd, const char *arg, switch_core_session_t *session, switch_stream_handle_t *stream)
{
	(void) session;
	(void) stream;
	oracle_record(ORACLE_API, cmd, arg);
	return SWITCH_STATUS_FALSE;
}

static _Thread_local switch_core_session_t oracle_session;
static _Thread_local switch_channel_t oracle_channel;
static _Thread_local switch_caller_extension_t oracle_extension;
static _Thread_local char *oracle_session_strings[8];
static _Thread_local int oracle_session_string_count;

static switch_status_t switch_ivr_originate(switch_core_session_t *session, switch_core_session_t **bleg, switch_call_cause_t *cause,
											const char *bridgeto, uint32_t timelimit_sec, void *table, const char *cid_name_override,
											const char *cid_num_override, void *caller_profile_override, void *ovars, int flags,
											void *cancel_cause, void *dh)
{
	char timeout[16];

	(void) session;
	(void) table;
	(void) caller_profile_override;
	(void) ovars;
	(void) flags;
	(void) cancel_cause;
	(void) dh;
	snprintf(timeout, sizeof(timeout), "%u", timelimit_sec);
	oracle_record(ORACLE_ORIGINATE, bridgeto, timeout);
	oracle_record(ORACLE_CALLER_ID, cid_name_override, cid_num_override);
	*cause = SWITCH_CAUSE_NORMAL_CLEARING;
	*bleg = &oracle_session;
	return SWITCH_STATUS_SUCCESS;
}

static switch_channel_t *switch_core_session_get_channel(switch_core_session_t *session)
{
	(void) session;
	return &oracle_channel;
}

static char *switch_core_session_strdup(switch_core_session_t *session, const char *todup)
{
	(void) session;
	if (oracle_session_string_count == sizeof(oracle_session_strings) / sizeof(oracle_session_strings[0])) {
		abort();
	}
	return oracle_session_strings[oracle_session_string_count++] = strdup(todup);
}

static switch_caller_extension_t *switch_caller_extension_new(switch_core_session_t *session, const char *name, const char *number)
{
	(void) session;
	(void) name;
	(void) number;
	return &oracle_extension;
}

static void switch_caller_extension_add_application(switch_core_session_t *session, switch_caller_extension_t *extension,
													 const char *application_name, const char *extra_data)
{
	(void) session;
	(void) extension;
	oracle_record(ORACLE_APPLICATION, application_name, extra_data);
}

static void switch_ivr_session_transfer(switch_core_session_t *session, const char *extension, const char *dialplan, const char *context)
{
	(void) session;
	oracle_record(ORACLE_TRANSFER, extension, dialplan);
	oracle_record(ORACLE_CONTEXT, context, NULL);
}

#define switch_channel_cause2str(cause) ((void) (cause), "CAUSE")
#define switch_channel_set_caller_extension(channel, extension) ((void) 0)
#define switch_channel_set_state(channel, state) ((void) 0)
#define switch_core_session_get_uuid(session) "uuid"
#define switch_core_session_rwunlock(session) ((void) 0)
"#;

const HARNESS: &str = r#"
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

int oracle_needs_url_encode(const char *s)
{
	return switch_needs_url_encode(s);
}

const char *oracle_url_unsafe(void)
{
	return SWITCH_URL_UNSAFE;
}

int oracle_switch_true(const char *expr)
{
	return switch_true(expr);
}

size_t oracle_core_url_encode_opt(const char *url, switch_bool_t double_encode, char *out, size_t outlen)
{
	switch_memory_pool_t pool = { { 0 }, 0 };
	char *encoded = switch_core_url_encode_opt(&pool, url, double_encode);
	size_t len = strlen(encoded);

	if (len < outlen) {
		memcpy(out, encoded, len + 1);
	} else {
		len = (size_t) -1;
	}
	while (pool.count) {
		free(pool.allocs[--pool.count]);
	}
	return len;
}

/* switch_event_create_brackets on a block opening data, as originate calls it: the offset after
   the block, or -1 where the call fails. */
long oracle_brackets(char *data, char a, char b, char c, oracle_emit_fn emit, void *ctx)
{
	switch_event_t event = { 0 };
	switch_event_t *var_event = &event;
	char *parsed = NULL;

	oracle_emit = emit;
	oracle_ctx = ctx;
	if (switch_event_create_brackets(data, a, b, c, &var_event, &parsed, SWITCH_FALSE) != SWITCH_STATUS_SUCCESS || !parsed) {
		return -1;
	}
	return (long) (parsed - data);
}

/* switch_ivr_originate from its first space strip to each leg's endpoint, the passes the port
   models, in its order; the statements between are the switch's own. */
static switch_status_t oracle_originate(const char *bridgeto)
{
	switch_status_t status = SWITCH_STATUS_SUCCESS;
	switch_core_session_t *session = NULL;
	switch_event_t event = { 0 };
	switch_event_t *var_event = &event;
	switch_event_t *local_var_event = NULL;
	char *pipe_names[MAX_PEERS] = { 0 };
	char *peer_names[MAX_PEERS] = { 0 };
	char *odata = strdup(bridgeto);
	char *data = odata;
	char *loop_data = NULL;
	char *chan_type = NULL;
	const char *failure = "Parse Error!";
	int or_argc = 0, and_argc = 0, r, i;

	(void) session;

	/* strip leading spaces */
	while (data && *data && *data == ' ') {
		data++;
	}

	if (switch_stristr("origination_nested_vars=true", data)) {
		oracle_record(ORACLE_NESTED, NULL, NULL);
	}

	ORACLE_ORIGINATE_ULTRA_GLOBAL

	ORACLE_ORIGINATE_GLOBAL

	/* strip leading spaces (again) */
	while (data && *data && *data == ' ') {
		data++;
	}

	if (zstr(data)) {
		failure = "No origination URL specified!";
		status = SWITCH_STATUS_GENERR;
		goto done;
	}

	loop_data = strdup(data);
	or_argc = switch_separate_string(loop_data, '|', pipe_names, (sizeof(pipe_names) / sizeof(pipe_names[0])));

	if (or_argc <= 0) {
		oracle_record(ORACLE_FAILURE, "Nothing to do", NULL);
		goto done;
	}

	for (r = 0; r < or_argc; r++) {
		char *p, *end = NULL;
		int q = 0, alt = 0;

		p = pipe_names[r];

		ORACLE_ORIGINATE_COMMA_SCAN

		and_argc = switch_separate_string(pipe_names[r], ',', peer_names, (sizeof(peer_names) / sizeof(peer_names[0])));
		oracle_record(ORACLE_GROUP, NULL, NULL);

		for (i = 0; i < and_argc; i++) {
			end = NULL;
			oracle_record(ORACLE_LEG, NULL, NULL);

			if (!(chan_type = peer_names[i])) {
				failure = "Empty dial string";
				switch_goto_status(SWITCH_STATUS_FALSE, done);
			}

			/* strip leading spaces */
			while (chan_type && *chan_type && *chan_type == ' ') {
				chan_type++;
			}

			if (*chan_type == '[') {
				switch_event_create_plain(&local_var_event, SWITCH_EVENT_CHANNEL_DATA);
			}

			ORACLE_ORIGINATE_LOCAL

			/* strip leading spaces (again) */
			while (chan_type && *chan_type && *chan_type == ' ') {
				chan_type++;
			}

			oracle_record(ORACLE_ENDPOINT, chan_type, NULL);

			if (local_var_event) {
				switch_event_destroy(&local_var_event);
			}
		}
	}

  done:
	if (status != SWITCH_STATUS_SUCCESS) {
		oracle_record(ORACLE_FAILURE, failure, NULL);
	}
	if (local_var_event) {
		switch_event_destroy(&local_var_event);
	}
	switch_safe_free(loop_data);
	switch_safe_free(odata);
	return status;
}

/* switch_ivr_enterprise_originate up to the thread split, each thread then read as
   switch_ivr_originate reads its bridgeto. */
static switch_status_t oracle_enterprise_originate(const char *bridgeto)
{
	switch_status_t status = SWITCH_STATUS_FALSE;
	switch_core_session_t *session = NULL;
	switch_event_t event = { 0 };
	switch_event_t *var_event = &event;
	char *x_argv[MAX_PEERS] = { 0 };
	char *odata = strdup(bridgeto);
	char *data = odata;
	const char *failure = "Parse Error!";
	int x_argc = 0, i;

	(void) session;

	/* strip leading spaces */
	while (data && *data && *data == ' ') {
		data++;
	}

	ORACLE_ENTERPRISE_ULTRA_GLOBAL

	/* strip leading spaces (again) */
	while (data && *data && *data == ' ') {
		data++;
	}

	if (!(x_argc = switch_separate_string_string(data, SWITCH_ENT_ORIGINATE_DELIM, x_argv, MAX_PEERS))) {
		failure = "DESTINATION_OUT_OF_ORDER";
		goto done;
	}

	for (i = 0; i < x_argc; i++) {
		oracle_record(ORACLE_THREAD, NULL, NULL);
		oracle_originate(x_argv[i]);
	}
	status = SWITCH_STATUS_SUCCESS;

  done:
	if (status != SWITCH_STATUS_SUCCESS) {
		oracle_record(ORACLE_FAILURE, failure, NULL);
	}
	switch_safe_free(odata);
	return status;
}

void oracle_dial(const char *bridgeto, oracle_emit_fn emit, void *ctx)
{
	oracle_emit = emit;
	oracle_ctx = ctx;
	if (strstr(bridgeto, SWITCH_ENT_ORIGINATE_DELIM)) {
		oracle_enterprise_originate(bridgeto);
	} else {
		oracle_record(ORACLE_THREAD, NULL, NULL);
		oracle_originate(bridgeto);
	}
}

void oracle_expand(const char *in, oracle_emit_fn emit, void *ctx)
{
	switch_channel_t channel = { 0 };
	char *expanded;

	oracle_emit = emit;
	oracle_ctx = ctx;
	expanded = switch_channel_expand_variables_check(&channel, in, NULL, NULL, 0);
	oracle_record(ORACLE_OUTPUT, expanded, NULL);
	if (expanded != in) {
		free(expanded);
	}
}

static switch_status_t oracle_stream_write(switch_stream_handle_t *handle, const char *fmt, ...)
{
	char line[4096];
	va_list ap;

	(void) handle;
	va_start(ap, fmt);
	vsnprintf(line, sizeof(line), fmt, ap);
	va_end(ap);
	oracle_record(ORACLE_OUTPUT, line, NULL);
	return SWITCH_STATUS_SUCCESS;
}

/* The api line after switch_api_execute strips its argument, run through originate_function. */
void oracle_api_originate(const char *arg, oracle_emit_fn emit, void *ctx)
{
	switch_stream_handle_t stream = { 0 };
	jmp_buf jump;
	char *stripped;

	oracle_emit = emit;
	oracle_ctx = ctx;
	stream.write_function = oracle_stream_write;
	stripped = switch_strip_whitespace(arg);
	if (!setjmp(jump)) {
		oracle_assert_jump = &jump;
		originate_function(stripped, NULL, &stream);
	}
	oracle_assert_jump = NULL;
	free(stripped);
	while (oracle_session_string_count) {
		free(oracle_session_strings[--oracle_session_string_count]);
	}
}
"#;

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

    let mut generated = String::new();
    for (number, tag) in TAGS
        .iter()
        .enumerate()
    {
        writeln!(generated, "const {tag}: c_int = {};", number + 1).expect("String write");
    }
    generated.push_str("static TREES: &[Tree] = &[\n");
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

    /// The first `#define` naming `name`, continuation lines included; a tree that moved it
    /// breaks the build rather than the oracle.
    fn define(&mut self, path: &'static str, name: &str) -> String {
        let commit = self
            .commit
            .clone();
        let lines: Vec<&str> = self
            .file(path)
            .lines()
            .collect();
        let start = lines
            .iter()
            .position(|line| {
                line.strip_prefix("#define ")
                    .and_then(|rest| rest.strip_prefix(name))
                    .is_some_and(|rest| rest.starts_with([' ', '\t', '(']))
            })
            .unwrap_or_else(|| panic!("{path} at {commit} defines no {name}"));
        let end = lines[start..]
            .iter()
            .position(|line| !line.ends_with('\\'))
            .map_or(lines.len() - 1, |at| start + at);
        format!("{}\n", lines[start..=end].join("\n"))
    }

    /// A function's first definition, from its signature at column 0 to the brace closing it there.
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

    /// The statement opening on the one line of `function` that reads `marker`, through the brace
    /// balancing its first.
    fn block(&mut self, path: &'static str, function: &str, marker: &str) -> String {
        let commit = self
            .commit
            .clone();
        let body = self.function(path, function);
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
            panic!(
                "{function} in {path} at {commit} reads {marker:?} on {} lines, not one",
                starts.len()
            );
        };
        let mut depth = 0usize;
        let mut lexer = Lexer::default();
        for (at, line) in lines
            .iter()
            .enumerate()
            .skip(start)
        {
            for c in line.chars() {
                match lexer.code(c) {
                    Some('{') => depth += 1,
                    Some('}') => {
                        depth -= 1;
                        if depth == 0 {
                            return format!("{}\n", lines[start..=at].join("\n"));
                        }
                    }
                    _ => {}
                }
            }
            lexer.end_line();
        }
        panic!("{marker:?} in {function} in {path} at {commit} never closes");
    }
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
                line[at + name.len()..].starts_with(['(', ')'])
                    && line[..at].ends_with([' ', '*', '('])
            })
}

fn compile(tree: &Tree, source: &mut Source<'_>, out: &Path) {
    let mut unit = String::new();
    for (number, tag) in TAGS
        .iter()
        .enumerate()
    {
        writeln!(unit, "#define ORACLE_{tag} {}", number + 1).expect("String write");
    }
    for symbol in FUNCTIONS
        .iter()
        .map(|&(_, name)| name)
        .chain(
            EXPORTS
                .iter()
                .copied(),
        )
    {
        writeln!(unit, "#define {symbol} {}_{symbol}", tree.name).expect("String write");
    }
    unit.push_str(PRELUDE);
    for &(path, name) in DEFINES {
        unit.push_str(&source.define(path, name));
    }
    for &(path, name) in FUNCTIONS {
        unit.push_str(&source.function(path, name));
    }
    let mut harness = HARNESS.to_owned();
    for &(path, function, marker, placeholder) in BLOCKS {
        let block = source.block(path, function, marker);
        assert!(
            harness.contains(placeholder),
            "the harness names no {placeholder}"
        );
        harness = harness.replace(placeholder, &block);
    }
    unit.push_str(&harness);
    let file = out.join(format!("{}.c", tree.name));
    fs::write(&file, unit).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
    // The unit is the switch's code as each tree ships it, so its warnings are not ours to fix.
    cc::Build::new()
        .warnings(false)
        .file(&file)
        .compile(&format!("freeswitch_{}", tree.name));
}
