# Contract: Widget Admin API (tenant dashboard surface)

Standard tenant-scoped routes: session auth + `X-Tenant-ID` + RBAC, mounted with the other tenant routes. Permissions: `WidgetsView` (reads) / `WidgetsManage` (writes) — Owner/Admin manage, Manager views, per existing role-mapping conventions. All writes audited (who/what/when) per constitution III. Envelope/pagination per `rest-api.md`.

## GET /tenant/widgets — [WidgetsView]

List widget instances (excludes soft-deleted).

- 200: `{ "data": [ …WidgetInstance… ] }` (cardinality is small; no pagination cursor needed, matches similar small-collection tenant endpoints)

**Naming note**: the same value is called `publicId` in these admin responses and `widgetId` in the public API's query/body parameters. This is intentional — admin responses distinguish it from the row `id`, while the public surface has only one identifier to name.

**WidgetInstance** (admin view):

```json
{
  "id": "…uuid…", "publicId": "wgt_…", "name": "Marketing site",
  "displayName": "Support", "primaryColor": "#4F46E5",
  "welcomeMessage": "Hi! How can we help?",
  "position": "bottom-right", "theme": "light",
  "enabled": true, "allowedDomains": ["example.com", "*.example.org"],
  "createdAt": "…", "updatedAt": "…"
}
```

## POST /tenant/widgets — [WidgetsManage]

Create an instance (US5-1). Body: `name` required; all appearance fields optional (defaults per data model). Server generates `publicId`.

- 201: WidgetInstance. 422 on validation failure (limits per data-model.md).

## GET /tenant/widgets/{id} — [WidgetsView]

- 200: WidgetInstance; 404 if absent/deleted or other-tenant.

## PUT /tenant/widgets/{id} — [WidgetsManage]

Full update of mutable fields (`name`, `displayName`, `primaryColor`, `welcomeMessage`, `position`, `theme`, `enabled`, `allowedDomains`). `publicId` is immutable.

- 200: updated WidgetInstance. Existing widget loads keep old config until next load (spec edge case).

## DELETE /tenant/widgets/{id} — [WidgetsManage]

Soft delete. Embedded widgets with this instance stop rendering on next load (config → 404 → silent).

- 204.

## GET /tenant/widgets/{id}/snippet — [WidgetsView]

Copyable embed snippet (FR-033), server-built so host/base URL logic stays in one place:

- 200:
  ```json
  { "data": { "snippet": "<script src=\"https://<public-host>/widget.js\" data-widget-id=\"wgt_…\" async></script>" } }
  ```

## Conversation attribution (existing endpoints, additive)

- `GET /tenant/conversations` rows and `GET /tenant/conversations/{id}` gain nullable `widgetInstance: { "id": "…", "name": "…" } | null` (FR-032/FR-018). Additive, backward-compatible.
- Widget conversations appear with `channel: "widget"` in existing inbox views; no new conversation endpoints.
