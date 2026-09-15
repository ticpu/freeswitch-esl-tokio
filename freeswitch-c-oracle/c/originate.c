/* switch_event_create_brackets and the passes switch_ivr_originate and
   switch_ivr_enterprise_originate run over a dial string up to each leg's endpoint. */
//@ define src/include/switch_types.h SWITCH_ENT_ORIGINATE_DELIM
//@ define src/switch_ivr_originate.c QUOTED_ESC_COMMA
//@ define src/switch_ivr_originate.c UNQUOTED_ESC_COMMA
//@ define src/switch_ivr_originate.c MAX_PEERS
//@ function src/switch_event.c switch_event_create_brackets

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

	//@ block src/switch_ivr_originate.c switch_ivr_originate if (*data == '<') {

	//@ block src/switch_ivr_originate.c switch_ivr_originate while (*data == '{') {

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

		//@ block src/switch_ivr_originate.c switch_ivr_originate while (p && *p) {

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

			//@ block src/switch_ivr_originate.c switch_ivr_originate while (*chan_type == '[') {

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

	//@ block src/switch_ivr_originate.c switch_ivr_enterprise_originate while (data && *data == '<') {

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
