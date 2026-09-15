# group_call output fixtures

Each file is the exact body of `api eval ${group_call(<group>@<domain>[+<flags>])}` from the live test switch, named `<group>.<flags>.txt`, where `none` means no flag suffix. `bgapi eval` returned the same bytes for every file. Bodies carry no trailing newline.

Captured on FreeSWITCH 1.10.13-dev (git 8bb2a39) against the `flattened-probe` directory groups described in [live-test-switch.md](../../../../docs/live-test-switch.md).

`pbx-calltakers.A.txt` is the same eval on a test PBX whose group members registered over both families, which the live test switch cannot produce: it carries bracketed IPv6 contacts, `transport=tcp` and `fs_nat=yes`. It was never originated. Its domain, gateway names and addresses were replaced with `pbx.example.com`, neutral names, `2001:db8:` prefixes and `192.0.2.x`, keeping each address's shape.

The live-switch captures were sanitised by this substitution and nothing else, applied in order:

| Captured | Committed |
|---|---|
| `127.0.0.1` | `192.0.2.1` |
| `[::1]` | `[2001:db8::1]` |
| `@default` at a word boundary | `@pbx.example.com` |
| `sip_invite_domain=default` | `sip_invite_domain=pbx.example.com` |

The captures carry the registering profile's SIP port, `5080`. A switch whose `lab-lo` profile listens on another port has that port read as `5080` before the substitution.
