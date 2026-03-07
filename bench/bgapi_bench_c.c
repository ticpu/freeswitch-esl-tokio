/*
 * bgapi throughput benchmark — C ESL
 *
 * Sends N `bgapi status` commands and collects all BACKGROUND_JOB results.
 * Measures send-phase latency and full round-trip.
 *
 * Environment variables:
 *   ESL_HOST      (default: localhost)
 *   ESL_PORT      (default: 8021)
 *   ESL_PASSWORD  (default: ClueCon)
 *   BENCH_COUNT   (default: 1000)
 *
 * Build: make -C bench FREESWITCH_SOURCE=/path/to/freeswitch
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <esl.h>

#define UUID_LEN 36
#define DEFAULT_HOST "localhost"
#define DEFAULT_PORT 8021
#define DEFAULT_PASS "ClueCon"
#define DEFAULT_N    1000

struct timing {
	struct timespec send_start;
	struct timespec recv_time;
	char job_uuid[UUID_LEN + 1];
	int received;
};

static double ts_to_us(struct timespec *a, struct timespec *b)
{
	double sec = (double)(b->tv_sec - a->tv_sec);
	double nsec = (double)(b->tv_nsec - a->tv_nsec);
	return (sec * 1e6) + (nsec / 1e3);
}

static double ts_diff_ms(struct timespec *a, struct timespec *b)
{
	double sec = (double)(b->tv_sec - a->tv_sec);
	double nsec = (double)(b->tv_nsec - a->tv_nsec);
	return (sec * 1e3) + (nsec / 1e6);
}

static int cmp_double(const void *a, const void *b)
{
	double da = *(const double *)a;
	double db = *(const double *)b;
	if (da < db) return -1;
	if (da > db) return 1;
	return 0;
}

static void print_latencies(const char *prefix, double *sorted, int n)
{
	if (n == 0) return;
	printf("%s_min_us=%.0f\n", prefix, sorted[0]);
	printf("%s_median_us=%.0f\n", prefix, sorted[n / 2]);
	printf("%s_p95_us=%.0f\n", prefix, sorted[(int)(n * 0.95)]);
	printf("%s_p99_us=%.0f\n", prefix, sorted[(int)(n * 0.99)]);
	printf("%s_max_us=%.0f\n", prefix, sorted[n - 1]);
}

static const char *env_or(const char *name, const char *def)
{
	const char *v = getenv(name);
	return (v && *v) ? v : def;
}

int main(void)
{
	const char *host = env_or("ESL_HOST", DEFAULT_HOST);
	int port = atoi(env_or("ESL_PORT", "8021"));
	const char *password = env_or("ESL_PASSWORD", DEFAULT_PASS);
	int n = atoi(env_or("BENCH_COUNT", "1000"));

	if (n <= 0 || n > 1000000) {
		fprintf(stderr, "BENCH_COUNT must be between 1 and 1000000\n");
		return 1;
	}

	struct timing *timings = calloc(n, sizeof(*timings));
	if (!timings) {
		perror("calloc");
		return 1;
	}

	esl_handle_t handle;
	memset(&handle, 0, sizeof(handle));

	if (esl_connect(&handle, host, (esl_port_t)port, NULL, password) != ESL_SUCCESS) {
		fprintf(stderr, "failed to connect to %s:%d\n", host, port);
		free(timings);
		return 1;
	}

	/* warmup */
	esl_send_recv(&handle, "api status\n\n");

	/* subscribe to BACKGROUND_JOB events */
	esl_events(&handle, ESL_EVENT_TYPE_PLAIN, "BACKGROUND_JOB");

	/* send phase */
	struct timespec send_phase_start, send_phase_end;
	clock_gettime(CLOCK_MONOTONIC, &send_phase_start);

	for (int i = 0; i < n; i++) {
		clock_gettime(CLOCK_MONOTONIC, &timings[i].send_start);
		esl_send_recv(&handle, "bgapi status\n\n");

		const char *uuid = NULL;
		if (handle.last_sr_event)
			uuid = esl_event_get_header(handle.last_sr_event, "Job-UUID");

		if (uuid && strlen(uuid) == UUID_LEN) {
			memcpy(timings[i].job_uuid, uuid, UUID_LEN);
			timings[i].job_uuid[UUID_LEN] = '\0';
		} else {
			fprintf(stderr, "warning: bgapi %d returned no Job-UUID\n", i);
			timings[i].job_uuid[0] = '\0';
		}
	}

	clock_gettime(CLOCK_MONOTONIC, &send_phase_end);

	/* receive phase: drain BACKGROUND_JOB events */
	struct timespec recv_phase_start, recv_phase_end;
	clock_gettime(CLOCK_MONOTONIC, &recv_phase_start);

	int received = 0;
	int timeout_count = 0;

	while (received < n && timeout_count < 10) {
		esl_status_t status = esl_recv_event_timed(&handle, 5000, 1, NULL);

		if (status != ESL_SUCCESS) {
			timeout_count++;
			continue;
		}

		if (!handle.last_ievent)
			continue;

		const char *uuid = esl_event_get_header(handle.last_ievent, "Job-UUID");
		if (!uuid)
			continue;

		struct timespec now;
		clock_gettime(CLOCK_MONOTONIC, &now);

		/* find matching send entry */
		for (int i = 0; i < n; i++) {
			if (!timings[i].received && strcmp(timings[i].job_uuid, uuid) == 0) {
				timings[i].recv_time = now;
				timings[i].received = 1;
				received++;
				timeout_count = 0;
				break;
			}
		}
	}

	clock_gettime(CLOCK_MONOTONIC, &recv_phase_end);

	esl_disconnect(&handle);

	/* compute send latencies (send_start to send_end approximated as next send_start) */
	double *send_lats = calloc(n, sizeof(double));
	for (int i = 0; i < n - 1; i++)
		send_lats[i] = ts_to_us(&timings[i].send_start, &timings[i + 1].send_start);
	if (n > 0)
		send_lats[n - 1] = ts_to_us(&timings[n - 1].send_start, &send_phase_end);
	qsort(send_lats, n, sizeof(double), cmp_double);

	/* compute round-trip latencies */
	double *rtts = calloc(n, sizeof(double));
	int rtt_count = 0;
	for (int i = 0; i < n; i++) {
		if (timings[i].received) {
			rtts[rtt_count++] = ts_to_us(&timings[i].send_start, &timings[i].recv_time);
		}
	}
	qsort(rtts, rtt_count, sizeof(double), cmp_double);

	double send_phase_ms = ts_diff_ms(&send_phase_start, &send_phase_end);
	double recv_phase_ms = ts_diff_ms(&recv_phase_start, &recv_phase_end);
	double total_ms = send_phase_ms + recv_phase_ms;

	printf("bench=c n=%d\n", n);
	printf("received=%d\n", received);
	printf("send_phase_ms=%.0f\n", send_phase_ms);
	printf("send_rate_per_sec=%.1f\n", (double)n / (send_phase_ms / 1000.0));
	print_latencies("send_lat", send_lats, n);
	printf("recv_phase_ms=%.0f\n", recv_phase_ms);
	if (rtt_count > 0)
		print_latencies("rtt", rtts, rtt_count);
	printf("total_ms=%.0f\n", total_ms);

	free(send_lats);
	free(rtts);
	free(timings);
	return 0;
}
