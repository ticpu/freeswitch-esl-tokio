/* switch_channel_str2cause from switch_channel.c and the table it reads. */
//@ declaration src/switch_channel.c switch_cause_table
//@ declaration src/switch_channel.c CAUSE_CHART
//@ function src/switch_channel.c switch_channel_str2cause

int oracle_str2cause(const char *str)
{
	return (int) switch_channel_str2cause(str);
}

/* Every named entry of CAUSE_CHART, in order, its number as the second string. */
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
