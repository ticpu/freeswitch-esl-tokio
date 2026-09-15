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
//@ export switch_separate_string_string (buf: *mut c_char, delim: *mut c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint
//@ function src/switch_utils.c separate_string_char_delim
//@ function src/switch_utils.c separate_string_blank_delim
//@ function src/switch_utils.c switch_separate_string
//@ export switch_separate_string (buf: *mut c_char, delim: c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint
//@ function src/switch_utils.c switch_find_end_paren
//@ export switch_find_end_paren (s: *const c_char, open: c_char, close: c_char) -> *mut c_char
//@ function src/switch_utils.c switch_stristr
//@ function src/switch_utils.c switch_strip_whitespace
//@ function src/switch_utils.c switch_is_number
//@ function src/include/switch_utils.h switch_true

//@ export oracle_cleanup (str: *mut c_char, delim: c_char) -> *mut c_char
char *oracle_cleanup(char *str, char delim)
{
	return cleanup_separated_string(str, delim);
}

//@ export oracle_char_delim (buf: *mut c_char, delim: c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint
unsigned int oracle_char_delim(char *buf, char delim, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_char_delim(buf, delim, array, arraylen);
}

//@ export oracle_blank_delim (buf: *mut c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint
unsigned int oracle_blank_delim(char *buf, char **array, unsigned int arraylen)
{
	memset(array, 0, arraylen * sizeof(*array));
	return separate_string_blank_delim(buf, array, arraylen);
}

//@ export oracle_switch_true (expr: *const c_char) -> c_int
int oracle_switch_true(const char *expr)
{
	return switch_true(expr);
}
