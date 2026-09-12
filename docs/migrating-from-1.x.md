# Migrating from 1.x

## Breaking changes in 2.0

- `Originate { endpoint, applications, .. }` struct literal: use the `Originate::application()`, `Originate::extension()` and `Originate::inline()` builders.
- `Endpoint::SofiaGateway { gateway, uri, .. }`: `Endpoint::SofiaGateway(SofiaGateway::new(gateway, uri))`.
- `Endpoint::Sofia { profile, uri, .. }`: `Endpoint::Sofia(SofiaEndpoint::new(profile, uri))`.
- `Endpoint::Loopback { extension, .. }`: `Endpoint::Loopback(LoopbackEndpoint::new(extension))`.
- `Endpoint::User { user, .. }`: `Endpoint::User(UserEndpoint::new(user))`.
- `HeaderLookup` typed accessors return `Result<Option<T>, _>` rather than `Option<T>`, with a parse error type per field, so a parse error is distinct from a missing header.
- `HeaderLookup` requires the `SipHeaderLookup` supertrait.
- `Variables::vars_type` is private; use the `scope()` accessor.
- `ChannelVariable` is renamed to `VariableName`.
- `linger(Option<u32>)` is replaced by `linger_timeout(Option<Duration>)`, deprecated in 1.x.

## New in 2.0

- **Serde support** for all command builders (feature-gated)
- **`EventSubscription`** unifies format/events/filters into reusable config
- **Typed endpoint builders** with `_mut()` accessors for deserialized configs
- **`EslResponse::api_result()`** convenience method
- **`getvar_opt()`** distinguishes unset variables from empty strings

## Upgrade steps

1. Replace endpoint struct literals with typed constructors (`SofiaGateway::new()`, etc.)
2. Replace `Originate` struct literals with builder methods
3. Add `SipHeaderLookup` supertrait to custom `HeaderLookup` impls
4. Handle `Result` wrapper on typed header accessors (add `?` or `.unwrap()`)
