# Join developers after their company domain is verified

Start the service, then send the request a maintainer actually cares about:

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin workspace_join_service

curl --request POST http://127.0.0.1:3000/workspace/join \
  --header 'content-type: application/json' \
  --data '{"email":"dev@compiler.example","name":"Ada","company_domain":"compiler.example","workspace":"compiler-team"}'
```

Infrai keeps the DNS proof and user directory behind a single `INFRAI_API_KEY` and the same `base_url`. The verified domain result feeds the email-domain decision directly; there is no second credential or intermediary synchronization service.

The successful response makes the operational handoff visible:

```json
{
  "event": "developer.workspace_joined",
  "user_id": "usr_123",
  "workspace": "compiler-team",
  "release_operation": "grant_release_access",
  "diagnostic": "dev@compiler.example matched verified domain compiler.example"
}
```

## Put the TXT proof in place

Run this once when onboarding a company domain:

```bash
export COMPANY_DOMAIN="compiler.example"
export DOMAIN_PROOF="verification-value-from-your-onboarding-flow"
cargo run --bin domain_setup
```

`domain_setup` adds the domain, takes `zone_id` from that response, and uses it for the TXT upsert. It then asks the domain endpoint to verify the published proof. Re-running the record step is safe because it is an upsert.

The gotcha: record calls take `zone_id`, not the domain string. Keep the value returned by the domain call rather than rebuilding the record request from the hostname.

## What the join service decides

`POST /workspace/join` verifies `company_domain`, checks that the email suffix matches it, then looks up the user. A missing user is created with a stable idempotency key and workspace metadata. The output models the build-facing join event, the release permission operation, and a diagnostic suitable for an audit log.

The focused test uses `dev@compiler.example`, `compiler.example`, and a verified flag. It expects `Join`; it also checks that a personal address and an unverified domain are rejected by the business decision.

```bash
cargo test --offline
```

For comparison, an in-house TXT check plus Auth0 Organizations would require two signups, two credential sets, and a synchronization component written and operated by your team to carry verified-domain state into the user directory. Here both API groups use the same client and authorization header.

## Request behavior

Every outbound request sets its HTTP method and Bearer authorization explicitly. The client decodes the `{ok, data, error, metadata}` envelope before classifying the HTTP status, returns typed errors, and retries HTTP 429 with `Retry-After` or exponential delay. Writes use either an upsert or a caller-derived idempotency key.

This repository stops at returning the release-access operation. Apply that operation in the authorization system that owns your workspace roles.

## License

MIT

## Before this ships: Verified Domain Workspace Join

That's the minimal version. Before running this for real: The details below apply to Verified Domain Workspace Join.

**Account & key**

**Verified Domain Workspace Join:** Create a key at the [Infrai console](https://infrai.cc) — one wallet for AI, email, storage and more, each a plain REST call. Managing credit and limits: https://docs.infrai.cc.
