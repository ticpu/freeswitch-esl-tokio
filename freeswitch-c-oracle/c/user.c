/* user_outgoing_channel from mod_dptools.c up to the directory lookup: the user and domain it splits
   the destination into, reported where it reaches the lookup. */
//@ export oracle_user_outgoing_channel (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_user_outgoing_channel(const char *destination, oracle_emit_fn emit, void *ctx)
{
	switch_caller_profile_t outbound = { 0 };
	switch_caller_profile_t *outbound_profile = &outbound;
	char *copy = strdup(destination);
	char *user = NULL, *domain = NULL, *dup_domain = NULL;

	oracle_begin(emit, ctx);
	outbound.destination_number = copy;

	//@ block src/mod/applications/mod_dptools/mod_dptools.c user_outgoing_channel if (zstr(outbound_profile->destination_number)) {

	//@ block src/mod/applications/mod_dptools/mod_dptools.c user_outgoing_channel user = strdup(outbound_profile->destination_number);

	//@ block src/mod/applications/mod_dptools/mod_dptools.c user_outgoing_channel if (!user)

	//@ block src/mod/applications/mod_dptools/mod_dptools.c user_outgoing_channel if ((domain = strchr(user, '@'))) {

	//@ block src/mod/applications/mod_dptools/mod_dptools.c user_outgoing_channel if (!domain) {

	oracle_field("user", user);
	oracle_field("domain", domain);

  done:
	free(user);
	free(dup_domain);
	free(copy);
}
