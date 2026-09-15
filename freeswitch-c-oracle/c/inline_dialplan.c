/* inline_dialplan_hunt from mod_dptools.c. */
//@ define src/include/switch_types.h SWITCH_STANDARD_DIALPLAN
#define switch_channel_get_caller_profile(channel) ((void) (channel), (switch_caller_profile_t *) NULL)
//@ function src/mod/applications/mod_dptools/mod_dptools.c inline_dialplan_hunt

/* The hunt over target with an empty destination number: each application it adds, then
   EXTENSION where it returns one. */
//@ export oracle_inline_dialplan_hunt (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_inline_dialplan_hunt(const char *target, oracle_emit_fn emit, void *ctx)
{
	switch_caller_profile_t profile = { 0 };
	char destination[1] = "";

	oracle_begin(emit, ctx);
	profile.destination_number = destination;
	profile.rdnis = "";
	if (inline_dialplan_hunt(NULL, (void *) target, &profile)) {
		oracle_record(ORACLE_EXTENSION, NULL, NULL);
	}
	oracle_session_release();
}
