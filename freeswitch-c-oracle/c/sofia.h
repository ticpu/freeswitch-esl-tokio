/* mod_sofia's profile and gateway, reduced to the fields its destination readers touch. A lookup
   is reported, and answers only for a name the harness lists. */
//@ typedef src/mod/endpoints/mod_sofia/mod_sofia.h sofia_transport_t

typedef struct sofia_profile {
	const char *name;
	char *domain_name;
	char *sipip;
	char *extsipip;
	int pres_type;
} sofia_profile_t;

typedef struct sofia_gateway {
	const char *name;
	sofia_profile_t *profile;
	int status;
	sofia_transport_t register_transport;
	int cid_type;
	char *register_from;
	char *destination_prefix;
	char *register_proxy;
	char *register_contact;
	char *from_domain;
	char *outbound_sticky_proxy;
	int ob_calls;
	int ob_failed_calls;
} sofia_gateway_t;

#define SOFIA_GATEWAY_UP 1
#define PFLAG_STANDBY 0
#define REG_FLAG_CALLERID 0
#define sofia_test_pflag(obj, flag) ((void) (obj), (void) (flag), 0)
#define sofia_test_flag(obj, flag) ((void) (obj), (void) (flag), 0)
#define sofia_glue_release_profile(profile) ((void) 0)
#define sofia_glue_profile_rdlock(profile) ((void) 0)

static _Thread_local const char *const *oracle_profiles;
static _Thread_local const char *const *oracle_gateways;
static _Thread_local sofia_profile_t oracle_profile;
static _Thread_local sofia_profile_t oracle_gateway_profile;
static _Thread_local sofia_gateway_t oracle_gateway;

static sofia_profile_t *sofia_glue_find_profile(const char *name)
{
	oracle_record(ORACLE_PROFILE, name, NULL);
	if (!oracle_known(oracle_profiles, name)) {
		return NULL;
	}
	memset(&oracle_profile, 0, sizeof(oracle_profile));
	oracle_profile.name = name;
	oracle_profile.sipip = "192.0.2.1";
	return &oracle_profile;
}

static sofia_gateway_t *sofia_reg_find_gateway(const char *name)
{
	oracle_record(ORACLE_GATEWAY, name, NULL);
	if (!oracle_known(oracle_gateways, name)) {
		return NULL;
	}
	memset(&oracle_gateway_profile, 0, sizeof(oracle_gateway_profile));
	oracle_gateway_profile.name = "gateway-profile";
	oracle_gateway_profile.sipip = "192.0.2.1";
	memset(&oracle_gateway, 0, sizeof(oracle_gateway));
	oracle_gateway.name = name;
	oracle_gateway.profile = &oracle_gateway_profile;
	oracle_gateway.status = SOFIA_GATEWAY_UP;
	oracle_gateway.register_transport = SOFIA_TRANSPORT_UDP;
	oracle_gateway.register_from = "<sip:gw@gateway.example.com>";
	oracle_gateway.destination_prefix = "";
	oracle_gateway.register_proxy = "sip:gateway.example.com";
	oracle_gateway.register_contact = "<sip:gw@192.0.2.1:5060>";
	return &oracle_gateway;
}
