/* protect_dest_uri from mod_sofia.c. */
//@ function src/mod/endpoints/mod_sofia/mod_sofia.c protect_dest_uri

/* The destination number the call leaves, then RESULT with what it returned. */
//@ export oracle_protect_dest_uri (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_protect_dest_uri(const char *destination, oracle_emit_fn emit, void *ctx)
{
	switch_memory_pool_t pool = { { 0 }, 0 };
	switch_caller_profile_t profile = { 0 };
	char *copy = strdup(destination);
	char result[16];

	oracle_begin(emit, ctx);
	profile.pool = &pool;
	profile.destination_number = copy;
	snprintf(result, sizeof(result), "%d", protect_dest_uri(&profile));
	oracle_record(ORACLE_OUTPUT, profile.destination_number, NULL);
	oracle_record(ORACLE_RESULT, result, NULL);
	free(copy);
	oracle_pool_release(&pool);
}
