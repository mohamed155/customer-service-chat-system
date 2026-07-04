# Helix UI tokens and naming

src/styles/tokens.css is the single source for the Helix light and dark palettes. The tokens cover application and panel surfaces (--bg-app, --panel*, --sidebar), borders (--border*), text hierarchy (--text*), accent foreground/backgrounds (--accent*), semantic status pairs (--green*, --amber*, --red*), and elevation (--shadow*). Set data-theme="dark" on the root element to select the extracted dark palette.

All component selectors use BEM with the hx- prefix: hx-block__element--modifier. Examples: hx-button--danger, hx-table__cell, and hx-tabs__tab--active. Component colors must reference these tokens, never literal colors.
