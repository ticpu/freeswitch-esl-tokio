/* switch_channel_expand_variables_check with every lookup answering nothing. */
//@ define src/switch_channel.c resize
//@ function src/switch_channel.c switch_channel_expand_variables_check

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
