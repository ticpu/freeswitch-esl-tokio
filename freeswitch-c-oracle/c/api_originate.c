/* originate_function from mod_commands.c on an API argument line. */
//@ define src/include/switch_types.h SWITCH_STANDARD_API
//@ define src/mod/applications/mod_commands/mod_commands.c ORIGINATE_SYNTAX
//@ function src/mod/applications/mod_commands/mod_commands.c originate_function

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
	oracle_session_release();
}
