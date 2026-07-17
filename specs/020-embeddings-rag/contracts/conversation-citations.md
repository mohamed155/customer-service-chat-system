# Contract: Conversation Citations (REST, additive to conversations module)

Additive, non-breaking extension of the conversation timeline contract. Existing consumers that ignore the new field are unaffected (Principle V).

## Additive field: citations on messages

The conversation timeline (`GET /tenant/conversations/{id}/messages`) and the `AddMessageResponse` `message` object gain a `citations` array.

```jsonc
{
  "id": "…",
  "kind": "ai",                 // citations only ever populated on AI-authored messages
  "body": "Our enterprise plan includes SSO and a dedicated CSM.",
  "created_at": "2026-07-17T10:00:00Z",
  "citations": [
    {
      "knowledge_item_id": "…",
      "item_title": "Enterprise plan overview",     // snapshot at response time
      "passage_text": "The enterprise plan includes SSO…",  // snapshot at response time
      "relevance_score": 0.83,
      "item_available": true      // false → source archived/deleted since; snapshot still renders
    }
  ]
}
```

## Contract rules

1. **Always present**: `citations` is always an array. It is empty (`[]`) for:
   - non-AI messages (customer/reply/note/system),
   - AI messages composed without retrieved knowledge (ungrounded).
   An empty array MUST render no citation affordance (FR-011) — visually identical to today's uncited responses.
2. **Snapshot semantics**: `item_title` and `passage_text` are point-in-time snapshots (FR-009). They render regardless of whether the source item still exists.
3. **`item_available`**: resolved by a live lookup at read time. `true` → the client MAY link to the current knowledge item detail (`/tenant/knowledge/items/{knowledge_item_id}`); `false` → client shows a "no longer available" indicator but still displays the snapshot (Story 2 acceptance #4).
4. **Ordering**: array is ordered by the stored `ordinal` (most-relevant first).
5. **No N+1**: the timeline handler batch-loads citations for all returned messages in one query.

## Frontend consumption (dashboard)

- Reusable `shared/components/citation-list` component renders the `citations` array as chips under an AI message; selecting a chip opens a preview/navigates to the item detail when `item_available`, else shows the snapshot with an unavailable badge (FR-011, Story 2 acceptance #2).
- Rendered only in the tenant dashboard conversation view; the customer widget is out of scope for v1 (spec Assumptions).

## Write path (internal)

Citations are written by the `ai` module in the same transaction as the AI reply insert (Phase C of the agent responder), via a `conversations::queries::insert_citations_in_tx` helper — respecting module ownership of the conversations schema (Principle I).
