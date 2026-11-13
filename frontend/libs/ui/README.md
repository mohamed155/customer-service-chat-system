# Helix UI (`libs/ui`)

Purpose: a small set of Helix design-system components (`hx-` prefix) and the Helix colour tokens,
written for the visual-system spec (003). **Nothing imports it today**: the dashboard builds its
screens from `apps/dashboard/src/app/shared/components` and Taiga UI, and the widget has its own
`--wgt-*` tokens. It is not built, and its specs (`components.spec.ts`) are not run by `ng test`;
only `pnpm lint` covers it. It is kept because it holds working code, not because it is part of the
app. The seven empty placeholder libraries that used to sit next to it were removed (MOH-63): new
dashboard code goes in `apps/dashboard/src/app` (`core/`, `shared/`, `layout/`, `features/`), not in
a lib.

## Helix UI tokens and naming

src/styles/tokens.css is the single source for the Helix light and dark palettes. The tokens cover application and panel surfaces (--bg-app, --panel*, --sidebar), borders (--border*), text hierarchy (--text*), accent foreground/backgrounds (--accent*), semantic status pairs (--green*, --amber*, --red*), and elevation (--shadow*). Set data-theme="dark" on the root element to select the extracted dark palette.

All component selectors use BEM with the hx- prefix: hx-block__element--modifier. Examples: hx-button--danger, hx-table__cell, and hx-tabs__tab--active. Component colors must reference these tokens, never literal colors.
