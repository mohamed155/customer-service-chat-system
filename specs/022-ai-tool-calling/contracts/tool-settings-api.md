# Contract: Tenant Tool Settings API

All routes under the authenticated tenant context (`X-Tenant-ID`, existing middleware). RBAC: read = Manager+; mutate = Admin+ (tenant), platform operators via existing tenant-switch rules. Standard error envelope; all mutations audited.

## `GET /tenant/tools`

Combined catalog view for the settings page.

```json
{
  "builtin": [
    {
      "name": "lookup_customer",
      "description": "Look up the conversation's customer profile",
      "classification": "auto",            // platform classification: "auto" | "approval"
      "enabled": true,                      // tenant policy (default false)
      "require_approval": false,            // tenant tightening flag
      "effective_approval": false           // derived: classification=="approval" || require_approval
    }
  ],
  "tenant_defined": [
    {
      "id": "uuid",
      "name": "check_order_status",
      "description": "...",
      "input_schema": { "type": "object", "properties": { } },
      "endpoint_url": "https://api.acme.example/tools/orders",
      "has_credential": true,               // credential itself is NEVER returned
      "classification": "approval",
      "enabled": true,
      "created_at": "...", "updated_at": "..."
    }
  ]
}
```

## `PUT /tenant/tools/builtin/{name}/policy`

Body: `{ "enabled": bool, "require_approval": bool }` → upserts the policy row. `422 tool_unknown` for names not in the catalog. Setting `require_approval: false` on a platform-`approval` tool is accepted but has no effect on `effective_approval` (tighten-only; response echoes the derived value so the UI can display it).

## `POST /tenant/tools` — register tenant-defined tool

Body:

```json
{
  "name": "check_order_status",
  "description": "...",
  "input_schema": { "type": "object", "properties": { } },
  "endpoint_url": "https://...",
  "credential": "secret-or-null",           // write-only
  "classification": "approval",             // optional; defaults to "approval"
  "enabled": true
}
```

Validation → `422`: name pattern / collision with built-in or live tenant tool; `input_schema` not a valid JSON Schema object; `endpoint_url` not HTTPS or failing the SSRF guard (loopback/private/link-local). Response: the `tenant_defined` item shape (`has_credential`, no secret echo).

## `PUT /tenant/tools/{id}`

Same body, all fields optional. `credential` semantics: absent = unchanged; `null` = cleared; string = replaced. Never echoed.

## `DELETE /tenant/tools/{id}`

Soft delete. Existing `tool_requests` referencing the tool remain intact and inspectable (FR-017); subsequent AI requests for the name are `refused`.

## Audit

Every mutation writes an audit record (existing conventions): actor membership, tenant, action (`tool.policy.updated`, `tool.created`, `tool.updated`, `tool.deleted`), and non-secret changed fields.
