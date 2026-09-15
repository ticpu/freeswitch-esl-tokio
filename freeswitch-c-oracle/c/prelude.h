/* The switch's types, reduced to what the extracted code touches. */
#include <setjmp.h>
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
