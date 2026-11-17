# Right-to-left layout

How the dashboard and the chat widget mirror for a right-to-left language (Arabic, Hebrew, Persian,
Urdu and others). Principle XII of the [constitution](../../.specify/memory/constitution.md) asks
for it.

## How the direction is chosen

The language decides. `LocaleService` (`core/i18n/`) picks the language at start-up and when it is
switched, and writes `lang` and `dir` on `<html>`: `rtl` for a right-to-left language, `ltr` for the
rest (`textDirection()` in `core/i18n/locale.ts`: the script of the language tag first, then the
language). Nothing else sets the direction, so a language and its direction cannot disagree. English
text is never shown right to left: a language with no catalog file falls back to English, and the
page stays left to right.

Taiga UI follows `[dir='rtl']` on the page by itself (its styles and its dropdown placement read it),
so there is nothing to configure for it.

The chat widget is a separate app and does the same: `I18n` (`apps/widget/src/i18n/`) sets `lang`
and `dir` on the widget's own document when it loads the language. Its `textDirection()` is a copy of
the dashboard's, tested the same way, because the two apps share no code. A widget's corner on the
host page (`bottom-right` or `bottom-left`) is a setting the tenant chooses, not the reading
direction, and does not flip.

## Writing a style that mirrors

Say where something goes relative to the reading direction, not the screen:

| Instead of | Write |
| -- | -- |
| `margin-left`, `margin-right` | `margin-inline-start`, `margin-inline-end` (`padding-`, `border-`, `scroll-margin-` likewise) |
| `left: 0`, `right: 0` | `inset-inline-start: 0`, `inset-inline-end: 0` |
| `text-align: left` / `right` | `text-align: start` / `end` |
| `border-top-left-radius` and the other three | `border-start-start-radius`, `border-start-end-radius`, `border-end-start-radius`, `border-end-end-radius` |
| `margin: 1px 4px 1px 0` (four values) | `margin-block: 1px; margin-inline: 0 4px` |
| `float: left` | `float: inline-start` |

Flexbox and grid already follow the direction (`justify-content: flex-end` is the end side), so a row
of buttons that sits at the end is already right.

A transform does not follow the direction. To slide or centre along the inline axis, scale the
distance by `--app-dir-sign` (`1` left to right, `-1` right to left, in `tokens.css`):

```css
inset-inline-start: 50%;
transform: translate(calc(-50% * var(--app-dir-sign)), -50%);
```

## Directional icons and arrows

- An icon whose shape points the way something goes (send, sign out) gets the class
  `app-flip-rtl`, which mirrors it when the page is right to left. Vertical chevrons and symmetric
  icons do not.
- An arrow in front of a "back" link, or behind a "forward" link, is not typed into the message.
  Put the class `app-arrow-back` or `app-arrow-forward` on the element, and write the message
  without the arrow: the stylesheet draws `←` or `→` and swaps them in a right-to-left page.
- A text separator such as `›` needs nothing: the browser mirrors it in a right-to-left page.

## What is checked

- `pnpm lint:rtl` (`frontend/tools/check-rtl.mjs`) fails CI on a physical property in the dashboard
  or the widget: the ones in the table, a four-value shorthand whose left and right differ,
  `translateX` without `--app-dir-sign`, and `left` or `right` in a position, an origin or a
  gradient. A line that must stay physical says why on the line:
  `rtl-lint-ignore: <why>` (the reason is required).
- `frontend/e2e/rtl.spec.ts` runs the shell, the inbox, a form dialog and the chat widget in English
  and in Arabic and compares where things are: the sidebar, the top bar, the list's avatar and time,
  the dialog's buttons and the visitor's chat bubble swap sides, and an arrow and a flipped icon
  change. Each check runs in both directions, so a layout that always flips or never flips fails.
  The tests attach a screenshot of each page to the report for a reviewer; no image is compared
  against a stored baseline, because it would differ between operating systems and fonts.

## Adding a language

Add the catalog files (`i18n/shared/<locale>.json`, one per area; see ADR-016). The direction comes
with the language: no code changes. Check the screens in that language before shipping it.
