# ADR-011: Widget as a lightweight Web Component with Shadow DOM, conversation in an isolated iframe

- **Status:** Accepted (MOH-17)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §4.2 and §6.3](../architecture/system-architecture.md#42-frontend-angular),
  ADR-009 (widget streams), ADR-010 (`gateway` serves the widget), `frontend/CLAUDE.md` (widget
  rules)

## Context

Tenants embed the chat widget on their own websites with one `<script>` tag. We don't control
those pages: they can have any CSS, any JavaScript framework, aggressive Content Security
Policies, slow devices, and sometimes hostile or compromised third-party scripts.

The decision has to balance these forces:

- **Page weight.** The widget loads on every page view of every tenant's site, including for
  visitors who never open it. Each kilobyte costs the tenant's page speed and SEO.
- **Style isolation, both ways.** Host CSS must not break the widget, and widget CSS must not leak
  into the host page.
- **Credential isolation.** The visitor's widget session token and conversation history must not
  be readable by the host page's scripts, which may include third-party ad or analytics code.
- **Compatibility.** Works on any framework (or none), in all evergreen browsers, without global
  name collisions.
- **Accessibility and theming.** Keyboard and screen-reader support; tenant branding (colours,
  position, launcher text).

## Decision

The widget has two parts with separate budgets and separate jobs.

### 1. Launcher: a Web Component with Shadow DOM on the host page

- The embed script defines one custom element, `<hx-chat-widget>`, and renders the launcher
  button, unread badge and the iframe container inside a **Shadow DOM** (`mode: 'closed'`).
  Shadow DOM keeps host CSS out and widget CSS in; the custom element avoids global name
  collisions.
- Written in **framework-free TypeScript** (or Lit if templating gets complex), no Angular.
- **Budget: ≤ 10 KB gzipped**, loaded `async`, no layout shift, served from the CDN with a
  versioned URL and long cache.
- It does only: fetch public widget config, render the branded launcher, create the iframe
  lazily on first open, and relay a small, validated `postMessage` protocol (open, close,
  resize, unread count). Messages are accepted only from the widget origin and with a known
  `source` and `type`.

### 2. Conversation UI: a lightweight app in an iframe on the widget origin

- The chat UI runs in an **iframe served from the widget origin** (the `gateway`, ADR-010), not in
  the host page's DOM. The iframe is created only when the visitor opens the widget.
- **Why not render the chat in the Shadow DOM too:** Shadow DOM isolates styles, not scripts.
  Any script on the host page runs in the same JavaScript realm and can read a shadow root's
  content, intercept `fetch`, or read the session token. An iframe on another origin is a real
  security boundary: the host page can't read its DOM, storage or network traffic.
- The iframe gets `sandbox="allow-scripts allow-same-origin allow-forms allow-popups"` (only what
  it needs), `referrerpolicy="strict-origin"`, and a CSP on the widget origin.
- The session token lives in the iframe origin's storage, never in the host page.
- **Budget: ≤ 50 KB gzipped initial** for the conversation UI. Framework-light (TypeScript or
  Lit, or Preact-sized). Larger features (attachments, rich cards) load lazily.

### 3. Budgets are enforced in CI

Both builds report gzipped size; CI fails if the launcher exceeds 10 KB or the conversation
initial bundle exceeds 50 KB.

### 4. Accessibility and theming

The launcher is a real `<button>` with an accessible name; focus moves into the iframe on open
and back to the launcher on close; `Escape` closes. Theming uses CSS custom properties set from
widget config, inside the shadow root and the iframe.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Everything in Shadow DOM on the host page** (no iframe) | Best page weight and no iframe resizing, but no script isolation: host scripts can read the conversation and the session token. Not acceptable for a product that carries customer conversations. |
| **Everything in an iframe, no Web Component** (plain DOM launcher) | Isolates the chat, but the launcher's styles and globals sit directly in the host page and can collide with host CSS. This is roughly the current state. |
| **Angular for the conversation UI** | Familiar to the team and shares patterns with the dashboard, but the runtime alone is close to the whole budget. The current Angular widget is about 59 KB gzipped before features are added. |
| **React / Vue widget** | Smaller than Angular but still heavier than needed, and adds a second framework to the codebase. |
| **Third-party chat SDK** (Intercom-style embed) | We are the chat product. |

## Consequences

**Positive**

- Tiny cost on host pages for visitors who never open the widget.
- Host CSS can't break the launcher, and host scripts can't read conversations or tokens.
- Works on any host framework.

**Negative and costs**

- Two builds with two budgets, plus a `postMessage` protocol to version and test.
- The conversation UI can't use the dashboard's Angular components; shared code is limited to
  types and the API client.
- Iframe resizing and focus handling across the boundary need care on mobile.
- Rewriting the conversation UI off Angular is real work (see W2).

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| W1 | **The launcher isn't a Web Component.** The loader appends a plain `<button>` and the iframe straight to `document.body` with inline styles. There's no custom element and no Shadow DOM, so host CSS can still affect the button. It already validates `postMessage` origin and `source`. | `frontend/apps/widget/loader/loader.ts` | Wrap the launcher in `<hx-chat-widget>` with a closed shadow root; add a size check to `pnpm build:widget-loader` (currently ~5 KB of source). |
| W2 | **The conversation UI is Angular** at about **59 KB gzipped** (`dist/widget/browser/main.js`, 187 KB raw), over the 50 KB target. | `frontend/apps/widget` | Rewrite in framework-light TypeScript or Lit as its own spec, keeping the existing API client and SSE contracts. Until then, don't add heavy dependencies. |
| W3 | **The iframe has no `sandbox` attribute.** | `loader.ts` (`toggleIframe`) | Add the sandbox and `referrerpolicy` listed above; test that chat, SSE and links still work. |
| W4 | **Budgets disagree.** `angular.json` allows 190 KB (warning) / 200 KB (error) for the widget's initial bundle, `frontend/CLAUDE.md` says 97 KB, and the architecture doc targets < 50 KB gzipped. | `frontend/angular.json`, `frontend/CLAUDE.md` | Make this ADR's numbers the source of truth; update `angular.json` and `frontend/CLAUDE.md` with W2. |
| W5 | The iframe and its token storage already sit on the widget origin, and the session token isn't exposed to the host page. This matches the decision. | `frontend/apps/widget/src/core/session.store.ts` | None. |
