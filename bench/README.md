# bgapi Benchmark: Rust vs C ESL

Measures bgapi throughput on a single ESL connection — sends N `bgapi status`
commands and collects all BACKGROUND_JOB results.

## Prerequisites

- FreeSWITCH running with ESL accessible (default: `localhost:8021`)
- `$FREESWITCH_SOURCE` pointing to the FreeSWITCH source tree (for C benchmark)

## Build

Rust (from repo root):

```sh
cargo build --release --example bgapi_bench
```

C:

```sh
make -C bench
```

## Run

```sh
# Rust
cargo run --release --example bgapi_bench

# C
./bench/bgapi_bench_c
```

## Configuration

All via environment variables (same for both):

- `ESL_HOST` — FreeSWITCH host (default: `localhost`)
- `ESL_PORT` — ESL port (default: `8021`)
- `ESL_PASSWORD` — ESL password (default: `ClueCon`)
- `BENCH_COUNT` — number of bgapi commands (default: `1000`)

## Output

Both produce the same greppable key=value format:

```
bench=rust n=1000
received=1000
send_phase_ms=2345
send_rate_per_sec=426.4
send_lat_min_us=1823
send_lat_median_us=2310
send_lat_p95_us=3012
send_lat_p99_us=4521
send_lat_max_us=8923
recv_phase_ms=1234
rtt_min_us=2345
rtt_median_us=3456
rtt_p95_us=5678
rtt_p99_us=7890
rtt_max_us=12345
total_ms=3579
```

## Caveats

- Network I/O variance: run multiple times for stable numbers
- FreeSWITCH load affects results — benchmark on an idle instance
- The Rust benchmark must be run with `--release` for a fair comparison
- ESL is a serial protocol — neither library can pipeline commands on a
  single connection. The async advantage is concurrent event buffering.
