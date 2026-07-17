# Runtime Contract: Prompt Rendering & Composition

**Feature**: 018-prompt-management | **Date**: 2026-07-16

How versioned prompt content reaches the LLM. Extends 017's `agent-runtime.md` pipeline; everything here is deterministic (Constitution IV) and unit-verified by byte-equality tests.

## Placeholder grammar (R4/R5)

- A placeholder is `{{name}}` where `name` matches `[a-z][a-z0-9_]*`.
- Lexing is a single left-to-right scan. Only the two-character sequences `{{` and `}}` are significant; single braces are literal prose.
- `{{` without a matching `}}`, `}}` without an opener, or a non-matching name between braces ⇒ `malformed_placeholder` (validation) / visible error highlight (preview). A well-formed `{{name}}` whose `name` is not in the catalog ⇒ `unknown_variable`.
- No escape syntax in v1 (deliberate — see research R5); no nesting; whitespace inside braces is not tolerated (`{{ agent_name }}` is malformed — keeps the grammar unambiguous and the client mirror trivial).

The same scanner semantics are implemented twice, by contract: `modules/ai::prompt_validate` (Rust, authoritative) and a pure TS mirror in the prompt feature (inline editor feedback + preview). A shared table-driven test fixture (same cases, same expected issues) keeps them aligned.

## Rendering

`render_prompt(content, vars) -> String`: single-pass replacement of each catalog placeholder with its resolved value. Deterministic: same `(content, vars)` ⇒ same bytes. Values are inserted verbatim (no markup, no re-scanning of inserted values — a customer named `{{agent_name}}` cannot inject a substitution).

Two callers, one function semantics:
- **Preview (client)**: `vars` = catalog samples; unresolved/malformed spans are rendered as highlighted error chips, never as normal text (FR-009).
- **Responder (server)**: `vars` = runtime values below; validation at save time guarantees no unknown/malformed placeholders exist in stored active content, so runtime rendering never encounters them (backfilled legacy content is the one exception — see Edge below).

## Runtime variable resolution (responder, per event)

| Variable | Source | Fallback (deterministic) |
|---|---|---|
| `agent_name` | agent config row name; platform persona name (`Assistant`) on the platform-AI branch | n/a — always present |
| `tenant_name` | `tenancy::authorize::fetch_tenant(tenant_id).name` (existing pub helper) | n/a — tenant row always exists |
| `customer_name` | conversations-owned pub helper returning the conversation's customer display name (small addition to the existing responder-helpers block in `conversations::queries` — Principle I: `ai` never reads conversations tables) | `the customer` (anonymous/absent name) |
| `channel` | outbox event payload (`conversation.customer_message` already carries `channel`) | n/a — always present |

Resolution adds at most one indexed read (customer display) beyond what the responder already fetches; `fetch_tenant` is one indexed PK read.

## Composition pipeline change (017 → 018)

```text
017: agent row (system_prompt column) ──────────────► compose_system_message(name, system_prompt, tone, rules)
018: active-content read (agent_prompts ⋈ versions) ─► render_prompt(content, runtime vars) ─► compose_system_message(name, rendered, tone, rules)
```

- The active-content read is the single query in [data-model.md](../data-model.md); absent prompt ⇒ empty string ⇒ `compose_system_message` omits the prompt section (unchanged 017 behavior).
- The platform-persona branch (unconfigured tenant, `ai_handling = 'platform_ai'`) passes empty content and performs **no** rendering — the persona has no tenant-authored prompt.
- `compose_system_message` itself is unchanged (prompt part first, tone directive, numbered rules, guardrail); its byte-equality determinism tests extend to cover rendered input.
- Version binding (FR-017): the responder reads `active_version` at run start; a save that lands mid-conversation binds at the next customer-message event — the same bind-at-next-run rule 017 defined for config changes.

## Edge: backfilled legacy content

Migration 0045 snapshots pre-018 prompt text as version 1 without validating it (it was legal under 017's rules). If such content happens to contain `{{`-like sequences, they are not catalog placeholders: runtime rendering replaces only well-formed catalog placeholders and leaves any other text byte-for-byte intact — so legacy prompts keep producing exactly the bytes they produced under 017. The editor will flag those spans on next open, and the next save/restore forces them through validation.

## Observability (Constitution VI)

The responder's existing structured trace events gain `prompt_version` (the bound `active_version`, or 0 for none/persona). Prompt content, rendered or raw, never appears in logs, traces, or audit details (015 invariant); the usage record keeps carrying only token counts.
