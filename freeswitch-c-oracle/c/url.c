/* The URL encoders of switch_utils.c. */
//@ define src/include/switch_utils.h SWITCH_URL_UNSAFE
//@ function src/include/switch_utils.h switch_needs_url_encode
//@ function src/switch_utils.c switch_url_encode_opt
//@ function src/switch_utils.c switch_url_encode
//@ function src/switch_utils.c switch_core_url_encode_opt

int oracle_needs_url_encode(const char *s)
{
	return switch_needs_url_encode(s);
}

const char *oracle_url_unsafe(void)
{
	return SWITCH_URL_UNSAFE;
}

size_t oracle_core_url_encode_opt(const char *url, switch_bool_t double_encode, char *out, size_t outlen)
{
	switch_memory_pool_t pool = { { 0 }, 0 };
	char *encoded = switch_core_url_encode_opt(&pool, url, double_encode);
	size_t len = strlen(encoded);

	if (len < outlen) {
		memcpy(out, encoded, len + 1);
	} else {
		len = (size_t) -1;
	}
	oracle_pool_release(&pool);
	return len;
}
