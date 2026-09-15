/* switch_channel_execute_on_value, which splits an execute_on_* value into an application and its
   argument, and the argument handling of switch_core_session_exec that follows it. */
//@ define src/include/switch_channel.h switch_channel_expand_variables

static switch_status_t switch_core_session_execute_application_async(switch_core_session_t *session, const char *app, const char *arg)
{
	char *ap, *arp;

	//@ block src/switch_core_session.c switch_core_session_execute_application_async if (!arg && strstr(app, "::")) {

	oracle_field("queued", "true");
	oracle_record(ORACLE_APPLICATION, app, arg);
	return SWITCH_STATUS_SUCCESS;
}

static switch_status_t switch_core_session_execute_application(switch_core_session_t *session, const char *app, const char *arg)
{
	//@ block src/switch_core_session.c switch_core_session_execute_application_get_flags if (!arg && strstr(app, "::")) {

	oracle_field("queued", "false");
	oracle_record(ORACLE_APPLICATION, app, arg);
	return SWITCH_STATUS_SUCCESS;
}

//@ function src/switch_channel.c switch_channel_execute_on_value

/* An execute_on_* value run as the hook runs it: FIELD queued, then APPLICATION with the name and
   the argument handed on, after what the argument's discarded expansion looked up. */
//@ export oracle_execute_on_value (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_execute_on_value(const char *value, oracle_emit_fn emit, void *ctx)
{
	switch_channel_t channel = { &oracle_session };

	oracle_begin(emit, ctx);
	switch_channel_execute_on_value(&channel, value);
	oracle_session_release();
}

/* The scope variables a %[ block sets are reported as pairs, as an event holding them is. */
static void switch_channel_set_scope_variables(switch_channel_t *channel, switch_event_t **event)
{
	(void) channel;
	if (*event) {
		switch_event_destroy(event);
	}
}

/* What switch_core_session_exec hands an application of its argument: PAIR for each scope variable
   a %[ block set, then APPLICATION with that argument. */
//@ export oracle_exec_argument (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_exec_argument(const char *arg, oracle_emit_fn emit, void *ctx)
{
	switch_core_session_t *session = &oracle_session;
	char *expanded = NULL;
	char delim = ',';
	int scope = 0;

	oracle_begin(emit, ctx);
	oracle_session.channel = &oracle_channel;

	//@ block src/switch_core_session.c switch_core_session_exec switch_bool_t expand_variables = !switch_true(switch_channel_get_variable(session->channel, "app_disable_expand_variables"));

	//@ block src/switch_core_session.c switch_core_session_exec if (arg) {

	//@ block src/switch_core_session.c switch_core_session_exec if (expand_variables && expanded && *expanded == '%' && (*(expanded+1) == '[' || *(expanded+2) == '[')) {

	(void) scope;
	oracle_record(ORACLE_APPLICATION, NULL, expanded);
	if (expanded != arg) {
		free(expanded);
	}
}
