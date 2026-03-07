FREESWITCH_SOURCE ?= $(error Set FREESWITCH_SOURCE to the FreeSWITCH source tree)
ESL_SRC = $(FREESWITCH_SOURCE)/libs/esl/src

CC ?= cc
CFLAGS ?= -O2 -Wall -Wextra
CPPFLAGS = -I$(ESL_SRC)/include
LDLIBS = -lpthread -lm

ESL_OBJS = esl.c esl_event.c esl_threadmutex.c esl_config.c \
           esl_json.c esl_buffer.c cJSON.c cJSON_Utils.c

bgapi_bench_c: bgapi_bench_c.c
	$(CC) $(CFLAGS) $(CPPFLAGS) -o $@ $< \
	  $(addprefix $(ESL_SRC)/,$(ESL_OBJS)) $(LDLIBS)

clean:
	rm -f bgapi_bench_c

.PHONY: clean
