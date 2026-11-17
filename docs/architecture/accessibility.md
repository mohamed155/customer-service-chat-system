# Accessibility checks

How the dashboard and the chat widget are kept to WCAG 2.2 AA (principle XII of the
[constitution](../../.specify/memory/constitution.md)). Lint rules catch what a template says;
these checks look at the pages as they render.

## Automated checks (`frontend/e2e/a11y.spec.ts`)

- **axe-core on the shell and the key pages, in both themes.** `@axe-core/playwright` runs on the
  overview, the inbox, a conversation, the knowledge base, the team page and the platform tenants
  page, each in the light and the dark theme, against the WCAG 2.0 to 2.2 A and AA rules. The test
  fails on a violation axe calls `serious` or `critical`, and names the rule, the elements and
  their HTML. It runs with the rest of `pnpm test:e2e` in CI.
- **A violation that is known has an issue.** `KNOWN` in the spec lists `{ page, theme?, rule,
  issue }`. Nothing is listed today. An entry hides one rule on one page, so list the smallest thing
  that is true and link the issue that removes it; delete the entry when it is fixed.
- **Reduced motion.** With `prefers-reduced-motion: reduce`, no element of the dashboard (including
  the sidebar drawer, menus and the loading spinner) has a transition or an animation longer than a
  blink; without it the spinner still moves, so the check is not vacuous. The chat widget has the
  same check on its send button. The rule is one global media query (`apps/dashboard/src/styles.css`,
  `apps/widget/src/index.html`); Taiga UI reads the preference for its own animations itself.
- **Loading, empty and page-container states in both themes** (spec 009 T073). For each state the
  text colour is measured against the colour it really sits on (every background above it blended
  together) and must be at least 4.5:1 in the light and the dark theme, and the same element, still
  on screen, must change its colours and stay legible when the system theme changes live.

## What the checks found, and the rules that follow

- The status colours as text (`--app-green`, `--app-amber`, `--app-red`) and the brand colour
  (`--app-accent`) are fills: they are not dark enough to read as text in the light theme. Text uses
  the `-text` tokens and `--app-accent-strong`; text on a solid accent uses `--app-accent-ink`. See
  [design tokens](design-tokens.md).
- A control's accessible name must contain the text people see on it (WCAG 2.5.3, axe rule
  `label-content-name-mismatch`): a button that shows the workspace name is named "Switch tenant:
  (the name)"; a button that shows only an avatar takes its name from visually hidden text (`.sr-only`),
  not from an `aria-label` that leaves the initials out.
- Every `<select>` and every link that has only an icon needs a name (`aria-label`).

## What they do not cover

axe finds what can be found by rules, about a third of the problems. It does not replace tabbing
through a page, a screen-reader pass, or checking a page at 200% zoom. Pages are checked in the state
the test reaches (loaded, with data); a dialog or menu that is closed is not checked. Add a page or a
state to `TARGETS` when it ships.
