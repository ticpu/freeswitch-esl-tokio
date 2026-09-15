/* The string tokenizer of switch_utils.c and the helpers the other units share. */
//@ define src/switch_utils.c ESCAPE_META
//@ define src/include/switch_utils.h end_of_p
//@ define src/include/switch_utils.h switch_goto_status
//@ define src/include/switch_utils.h switch_safe_free
//@ define src/include/switch_types.h SWITCH_BLANK_STRING
//@ function src/include/switch_utils.h _zstr
//@ function src/include/switch_utils.h switch_toupper
//@ function src/include/switch_utils.h switch_strchr_strict
//@ function src/include/switch_utils.h switch_string_has_escaped_data
//@ function src/include/switch_utils.h switch_string_var_check_const
//@ function src/switch_utils.c unescape_char
//@ function src/switch_utils.c cleanup_separated_string
//@ function src/switch_utils.c switch_separate_string_string
//@ function src/switch_utils.c separate_string_char_delim
//@ function src/switch_utils.c separate_string_blank_delim
//@ function src/switch_utils.c switch_separate_string
//@ function src/switch_utils.c switch_find_end_paren
//@ function src/switch_utils.c switch_stristr
//@ function src/switch_utils.c switch_strip_whitespace
//@ function src/switch_utils.c switch_is_number
//@ function src/include/switch_utils.h switch_true

char *oracle_cleanup(char *str, char delim)
{
	return cleanup_separated_string(str, delim);
}

unsigned int oracle_char_delim(char *buf, char delim, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_char_delim(buf, delim, array, arraylen);
}

unsigned int oracle_blank_delim(char *buf, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_blank_delim(buf, array, arraylen);
}

int oracle_switch_true(const char *expr)
{
	return switch_true(expr);
}
