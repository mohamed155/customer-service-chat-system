# ADR-016: Internationalization: runtime message catalogs

- **Status:** Accepted (MOH-41)
- **Date:** 2026-11-12
- **Deciders:** Platform engineering, product owner (the runtime-catalog choice, 2026-11-11)
- **Related:** constitution Principle XII ("user-facing text MUST be externalized"), ADR-003
  (frontend structure), ADR-011 (widget), ADR-012 (error contract)

## Context

The dashboard and the widget were built before the rule, and their text is written straight into
templates and components: about 123 HTML templates and 17 inline templates, roughly 1,000 strings.
Adding a language today means editing every component. The backend returns free-text messages for
some errors, so a client cannot show those in another language either.

The decision has to balance these forces:

- **Adding a language should not need code.** A translator should be able to add a catalog file.
- **No rebuild per language.** One build and one deployment serve every language.
- **The widget is small on purpose.** It runs inside other people's pages with a size budget and no
  third-party runtime libraries (ADR-011).
- **Plurals, dates and numbers differ by language,** and "1 stars" is a bug in any language.
- **Backend text is not a UI string.** A client shows its own wording for an error `code`.

## Decision

### 1. Runtime message catalogs, not build-time translation

Text lives in JSON catalogs loaded at run time. Angular's built-in i18n (`$localize`, one build per
locale) is rejected: every language becomes a separate build artifact and deployment, and "adding a
language needs no code change" would only mean "needs a new build".

### 2. Dashboard: Transloco

- `@jsverse/transloco` with the English catalog as the default and fallback. A feature area is a
  Transloco *scope*, loaded with its lazy route, so the initial bundle holds only the shared and
  auth catalogs.
- Catalog files: `apps/dashboard/public/i18n/<scope>/<locale>.json`. The `shared` scope is the
  default catalog: its English is part of the build (no request to show the first screen) and
  every other language is the file `shared/<locale>.json`. A new locale is a new file per scope;
  no code changes. A language is offered when its `shared` file loads: the active one is the
  person's own choice, then the browser's languages in order, then English. Nothing in the API
  stores a per-user language yet, so the choice is kept in the browser (`app.locale`).
- Plurals, select and ordinals use **ICU MessageFormat**, read by a small interpreter of our own
  (`core/i18n/icu.ts`, about 2 kB) on `Intl.PluralRules`, `Intl.NumberFormat` and
  `Intl.DateTimeFormat`. `@jsverse/transloco-messageformat` is not used: it builds a function from
  every message with the `Function` constructor, which the dashboard's policy forbids
  (`script-src 'self'` has no `unsafe-eval`, and Trusted Types refuses a string turned into code,
  MOH-170); allowing it would undo that work. The message syntax is the same, so a catalog can move
  to another ICU implementation without change. Dates and numbers go through `Intl` with the
  active locale (`localeDate`, `localeNumber`, `relativeTime`), never a hand-built format and not
  Angular's `DatePipe`, which needs locale data registered in code for each language.
- Taiga UI's own text (calendar, validation messages) follows the same locale through Taiga's
  `TUI_LANGUAGE`, from `@taiga-ui/i18n` for the languages Taiga ships (each a lazy chunk); a
  language Taiga lacks keeps English for Taiga's text only.

### 3. Widget: a small lookup of its own

The widget has no library. `I18n` (`apps/widget/src/i18n`) looks keys up in a catalog: English is
bundled in code, another locale is `i18n/<locale>.json` beside the widget, fetched at run time
(`?lang=<locale>` on the widget URL, else the browser's language). A missing key falls back to
English, then to the key. Plural forms use `Intl.PluralRules`. A translation file is treated as
data: only strings and plural-form objects are read from it.

### 4. Keys and catalogs

- A key is `area.element` (`chat.close`, `feedback.thanks`), named for what the text is, not for
  its wording. One key per message; a sentence is never built from pieces, and parameters are named
  (`{value}`), never positional.
- English is the source and is always complete; a missing translation falls back to English.
- `public/i18n/en.json` of the widget is shipped for translators to copy, and a test keeps it equal
  to the bundled catalog.

### 5. Errors are shown by `code`, not by the backend's text

A client maps an error's `code` to a catalog key and shows that (`errorMessageKey`, `core/errors/error-message.ts`; a field error in `details` goes through its own `code` the same way). The backend `message` is for
logs and developers, and is never shown as it is. An unknown code shows a generic message. The
backend keeps its stable `code` set (ADR-012); free text may change without breaking a client.

### 6. Enforcement

`pnpm lint:i18n` (CI job `frontend`) scans templates for hard-coded text (text nodes and literal
`placeholder`, `title`, `aria-label`, `alt` ... values) and fails a file that gains any. It is a
ratchet: `frontend/tools/i18n-baseline.json` records what is still written in place per file; the
number may only go down, and a file that improves must lower its baseline. It does not read message
strings in TypeScript; reviewers check those.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Angular built-in i18n** | One build per locale; extraction and merge tooling for XLIFF; the product owner chose a runtime catalog. |
| **ngx-translate** | Comparable to Transloco but less maintained, and without scoped lazy loading as a first-class feature. |
| **`@jsverse/transloco-messageformat`** | It compiles each message with `new Function`, blocked by the dashboard's CSP and Trusted Types (MOH-170). |
| **Transloco in the widget** | The widget's size budget and its no-library rule (ADR-011). |
| **A single hand-written service for both apps** | Duplicates a maintained library's plural, scope and loader work in the dashboard for no gain; the widget is small enough that its own lookup is cheaper than a library. |

## Consequences

**Positive**

- A language is a file; the widget and the dashboard need no rebuild for it.
- New hard-coded text fails CI, so the gap cannot grow.
- A fix to an error's wording is a catalog edit, not an API change.

**Negative and costs**

- A catalog request at start-up for a non-English locale (English needs none), and a failed request for each browser language that has no file (the server answers 404 and the page goes on in English).
- The initial bundle grew by about 37 kB raw (11 kB transferred): Transloco, the message reader and the shared English catalog. Taiga's language files add about 200 kB of lazy chunks.
- A message reader of our own to maintain, covering only the ICU features the catalogs use (a test keeps every English message readable).
- A migration of about 740 strings in 83 dashboard files, staged by area (below).
- Reviewers still have to catch message strings in TypeScript, which the check does not read.
- The widget's initial size grew by about 3 kB for the lookup (its warning threshold moved from 190
  to 195 kB; the 200 kB limit is unchanged).

## Current state and migration

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| I1 | **The widget** had about 20 hard-coded strings. | `frontend/apps/widget/src` | **Resolved (MOH-41):** all moved to `I18n`; `?lang=` and `i18n/<locale>.json` work; plural fixed ("1 star"). |
| I2 | **No check for new hard-coded text.** | CI | **Resolved (MOH-41):** `pnpm lint:i18n` with a per-file baseline (744 texts in 83 dashboard files at the time of writing). |
| I3 | **The dashboard has no catalog mechanism** and about 740 strings in templates. | `frontend/apps/dashboard/src/app` | **Mechanism and first area resolved (MOH-155):** Transloco, the catalogs, language selection, `Intl` dates and numbers, Taiga's language, the checks in `pnpm lint:i18n` and the test setup; `shared/`, `layout/`, `not-found` and `tenant-select` moved (670 texts in 62 files remain). The feature areas follow, each lowering the baseline (MOH-156 to MOH-165). |
| I4 | **Backend free-text error messages** (about 65 sites) are shown by some screens. | `backend/crates/**`, dashboard error screens | **Mechanism and the sign-in area resolved (MOH-156):** `errorMessageKey` and `fieldErrorKey` map a `code` to a catalog key, a test fails when the backend adds a code (kernel, identity, invitation) that is not listed, and the auth screens, the step-up prompt and the invitation page show no backend text (a disabled membership got its own code, `membership_disabled`, instead of being told apart by its message). The other screens move with their area (MOH-157 to MOH-165); `userMessageFor` (English only) goes with the last. |
| I5 | **Message strings in TypeScript** are not scanned. | components and services | Moved with their area; reviewed by hand. |
