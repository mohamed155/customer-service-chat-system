/**
 * The widget's English messages, the default and the fallback for every other locale.
 *
 * Keys are `area.element`. A value is a string, or an object of plural forms (`zero`, `one`, `two`,
 * `few`, `many`, `other`, as `Intl.PluralRules` names them; `other` is required). `{name}` marks a
 * parameter. Never build a sentence from pieces: give it one key with parameters.
 *
 * To add a locale, copy `public/i18n/en.json` to `public/i18n/<locale>.json` and translate the
 * values. The widget loads it at run time (`?lang=<locale>`), so no code changes and no rebuild
 * of the widget are needed beyond shipping the file.
 */
export const EN = {
  'chat.title': 'Chat',
  'chat.windowLabel': 'Chat window',
  'chat.close': 'Close chat',
  'chat.error': 'Something went wrong.',
  'chat.retry': 'Try again',
  'chat.rateLimited': "You're sending messages too fast. Please slow down.",
  'composer.placeholder': 'Type your message…',
  'composer.label': 'Message input',
  'composer.send': 'Send message',
  'handoff.connecting': 'Connecting you to a support agent…',
  'handoff.away': "Our team is away — we'll reply as soon as someone is back.",
  'typing.label': 'Assistant is typing',
  'feedback.heading': 'How did we do?',
  'feedback.commentPlaceholder': 'Tell us more (optional)',
  'feedback.send': 'Send feedback',
  'feedback.dismiss': 'Not now',
  'feedback.rate': 'Rate this conversation',
  'feedback.thanks': 'Thank you for your feedback!',
  'rating.group': 'Rating: {value} out of 5',
  'rating.star': { one: '{count} star', other: '{count} stars' },
} as const satisfies Catalog;

export type MessageKey = keyof typeof EN;

export type PluralForms = {
  readonly zero?: string;
  readonly one?: string;
  readonly two?: string;
  readonly few?: string;
  readonly many?: string;
  readonly other: string;
};
export type Entry = string | PluralForms;
export type Catalog = Readonly<Record<string, Entry>>;
export type Params = Readonly<Record<string, string | number>>;
