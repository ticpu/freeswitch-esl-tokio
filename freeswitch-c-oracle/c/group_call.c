/* group_call_function from mod_commands.c up to the directory lookup: the group, domain and call
   delimiter it reads from its argument, reported where it reaches the lookup. */
//@ export oracle_group_call (input: *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_group_call(const char *cmd, oracle_emit_fn emit, void *ctx)
{
	char *domain = NULL, *dup_domain = NULL;
	char *group_name = NULL;
	char *flags;
	char *fp = NULL;
	const char *call_delim = ",";

	oracle_begin(emit, ctx);

	//@ block src/mod/applications/mod_commands/mod_commands.c group_call_function if (zstr(cmd)) {

	//@ block src/mod/applications/mod_commands/mod_commands.c group_call_function group_name = strdup(cmd);

	//@ block src/mod/applications/mod_commands/mod_commands.c group_call_function if ((flags = strchr(group_name, '+'))) {

	//@ block src/mod/applications/mod_commands/mod_commands.c group_call_function domain = strchr(group_name, '@');

	//@ block src/mod/applications/mod_commands/mod_commands.c group_call_function if (domain) {

	oracle_field("group", group_name);
	oracle_field("domain", domain);
	oracle_field("call_delim", call_delim);

  end:
	free(group_name);
	free(dup_domain);
}
