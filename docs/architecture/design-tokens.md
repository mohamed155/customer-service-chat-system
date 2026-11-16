# Design tokens

How the dashboard gets its colors, and what to use instead of writing one. Principle XII of the
[constitution](../../.specify/memory/constitution.md) asks for one visual system, in light and dark.

## The rule

A color on a dashboard screen is always a token: `var(--app-…)`. A hex value, `rgb()`, `rgba()`,
`hsl()`, a color name (`white`) or a fallback beside a token (`var(--app-x, #fff)`) is not allowed
outside `frontend/apps/dashboard/src/app/design-system/`, the one place a color value is written.
`pnpm lint:colors` (`frontend/tools/check-colors.mjs`) fails CI on a new one. The same check fails
a `var(--app-…)` that the design system does not define in both themes: a misspelled or invented
token falls back to nothing, and the screen goes wrong in one theme.

The token files:

- `design-system/theme/themes.css`: the themed tokens, defined once for the light theme
  (`:root[data-theme='light']`) and once for the dark one. A token must exist in both.
- `design-system/tokens/tokens.css`: tokens that do not change with the theme (spacing, radius, type,
  motion, chart series colors, the widget preview palette).

## Which token

| For | Use |
| -- | -- |
| Page and card backgrounds | `--app-bg`, `--app-panel`, `--app-panel-2`, `--app-panel-3`, `--app-sidebar` |
| Hover fill | `--app-fill-hover` |
| Borders | `--app-border`, `--app-border-strong` |
| Text | `--app-text` (body), `--app-text-2` (secondary), `--app-text-3` (hints) |
| Brand and links | `--app-accent`, `--app-accent-strong` (text on a light surface), `--app-accent-soft` (tinted background), `--app-accent-ink` (text on a solid accent) |
| Success | `--app-green`, `--app-green-soft`, `--app-green-ink` |
| Warning | `--app-amber`, `--app-amber-soft`, `--app-amber-ink` |
| Error and danger | `--app-red`, `--app-red-soft`, `--app-red-ink` |
| A dimmed page behind a dialog | `--app-scrim` |
| Elevation | `--app-shadow` (card), `--app-shadow-lg` (menu, dialog) |
| Chart series | `--app-chart-1`, `--app-chart-2` |

Solid and soft: a *solid* color (`--app-red`) is a filled background, and the text on it is its `ink`
(`--app-red-ink`, which is dark in the dark theme where the solid is light). A *soft* color
(`--app-red-soft`) is a tint over a panel, and the text on it is the plain color or `--app-text`.
Never put a fixed `#fff` or `#000` on a themed color: it is right in one theme only.

Spacing, radius, type and motion tokens (`--app-space-*`, `--app-radius-*`, `--app-font-*`,
`--app-transition-*`) are in `tokens.css`.

## Adding a token

Add one only when no token above fits: a real need, not a shade you prefer. Put a themed token in
**both** blocks of `themes.css`, a fixed one in `tokens.css`, add it to the table above, and check
the screen in light and dark (the theme toggle is in the topbar).

## A value that is not a dashboard color

A color a tenant stores is data, not styling: the default brand color of a new chat widget
(`features/tenant/widgets/widget-defaults.ts`) is one. Mark that line with
`color-lint-ignore: <why>`. The reason is required. A whole file that cannot use tokens is listed in
`EXEMPT` in `check-colors.mjs` with its reason (today only `index.html`, the loading skeleton shown
before the stylesheet loads).
