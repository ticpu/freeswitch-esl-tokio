/* switch_channel_str2cause from switch_channel.c and the table it reads. */
//@ declaration src/switch_channel.c switch_cause_table
//@ declaration src/switch_channel.c CAUSE_CHART
//@ function src/switch_channel.c switch_channel_str2cause

//@ export oracle_str2cause (str: *const c_char) -> c_int
int oracle_str2cause(const char *str)
{
	return (int) switch_channel_str2cause(str);
}

/* Every named entry of CAUSE_CHART, in order, its number as the second string. */
//@ export oracle_cause_chart (emit: Emit, ctx: *mut c_void)
void oracle_cause_chart(oracle_emit_fn emit, void *ctx)
{
	size_t x;
	char number[16];

	oracle_begin(emit, ctx);
	for (x = 0; x < sizeof(CAUSE_CHART) / sizeof(CAUSE_CHART[0]) && CAUSE_CHART[x].name; x++) {
		snprintf(number, sizeof(number), "%d", (int) CAUSE_CHART[x].cause);
		oracle_record(ORACLE_CAUSE, CAUSE_CHART[x].name, number);
	}
}
