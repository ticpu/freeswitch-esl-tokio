/* sofia_contact_function from mod_sofia.c with no session, the profile hash empty and every
   registration query reported as SELECT, then its arguments as fields, returning no contact. */
//@ define src/include/switch_utils.h end_of

typedef struct switch_hash_index switch_hash_index_t;
static struct {
	void *hash_mutex;
	void *profile_hash;
} mod_sofia_globals;
#define switch_mutex_lock(mutex) ((void) 0)
#define switch_mutex_unlock(mutex) ((void) 0)
#define switch_core_hash_first(hash) ((void) (hash), (switch_hash_index_t *) NULL)
#define switch_core_hash_next(hi) ((void) (hi), (switch_hash_index_t *) NULL)
#define switch_core_hash_this(hi, key, klen, val) ((void) 0)

static void select_from_profile(sofia_profile_t *profile, const char *user, const char *domain, const char *concat,
								const char *exclude_contact, const char *match_user_agent, switch_stream_handle_t *stream,
								switch_bool_t dedup)
{
	(void) stream;
	oracle_record(ORACLE_SELECT, profile->name, dedup ? "true" : "false");
	oracle_field("user", user);
	oracle_field("domain", domain);
	oracle_field("concat", concat);
	oracle_field("exclude_contact", exclude_contact);
	oracle_field("match_user_agent", match_user_agent);
}

//@ function src/mod/endpoints/mod_sofia/mod_sofia.c sofia_contact_function

//@ export oracle_sofia_contact (arg: *const c_char, profiles: *const *const c_char, emit: Emit, ctx: *mut c_void)
void oracle_sofia_contact(const char *arg, const char *const *profiles, oracle_emit_fn emit, void *ctx)
{
	switch_stream_handle_t stream = { 0 };
	jmp_buf jump;

	oracle_begin(emit, ctx);
	oracle_profiles = profiles;
	stream.write_function = oracle_stream_write;
	if (!setjmp(jump)) {
		oracle_assert_jump = &jump;
		sofia_contact_function(arg, NULL, &stream);
	}
	oracle_assert_jump = NULL;
	oracle_profiles = NULL;
}
