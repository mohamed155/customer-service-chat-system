import { formatMessage, parseMessage } from './icu';

const fmt = (message: string, params: Record<string, unknown> = {}, locale = 'en') =>
  formatMessage(parseMessage(message), locale, params);

describe('ICU messages', () => {
  it('returns plain text and fills named values', () => {
    expect(fmt('Hello')).toBe('Hello');
    expect(fmt('Hello {name}!', { name: 'Ada' })).toBe('Hello Ada!');
  });

  it('picks the plural form of the locale, an exact value first, and writes # as the number', () => {
    const message = '{n, plural, =0 {No messages} one {# message} other {# messages}}';
    expect(fmt(message, { n: 0 })).toBe('No messages');
    expect(fmt(message, { n: 1 })).toBe('1 message');
    expect(fmt(message, { n: 1234 })).toBe('1,234 messages');
    // Polish has more forms than English, and the right one comes from the locale.
    const polish = '{n, plural, one {# plik} few {# pliki} many {# plików} other {# pliku}}';
    expect(fmt(polish, { n: 2 }, 'pl')).toBe('2 pliki');
    expect(fmt(polish, { n: 5 }, 'pl')).toBe('5 plików');
  });

  it('supports offset, ordinal and select, nested', () => {
    expect(
      fmt('{n, plural, offset:1 =0 {Nobody} =1 {Just you} other {You and # others}}', { n: 3 }),
    ).toBe('You and 2 others');
    expect(fmt('{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}', { n: 22 })).toBe(
      '22nd',
    );
    const nested =
      '{who, select, bot {AI} other {{n, plural, one {# agent} other {# agents}}}} replied';
    expect(fmt(nested, { who: 'bot', n: 1 })).toBe('AI replied');
    expect(fmt(nested, { who: 'human', n: 2 })).toBe('2 agents replied');
  });

  it('formats numbers and dates for the locale', () => {
    expect(fmt('{n, number}', { n: 1234.5 })).toBe('1,234.5');
    expect(fmt('{n, number}', { n: 1234.5 }, 'de')).toBe('1.234,5');
    expect(fmt('{n, number, percent}', { n: 0.25 })).toBe('25%');
    expect(fmt('{n, number, integer}', { n: 2.7 })).toBe('3');
    expect(fmt('{d, date, long}', { d: new Date('2026-06-16T12:00:00Z') }, 'en')).toContain('2026');
    expect(fmt('{d, time, short}', { d: new Date('2026-06-16T12:00:00Z') })).toMatch(/\d/);
  });

  it('keeps apostrophes and quotes braces the ICU way', () => {
    expect(fmt("You're all caught up")).toBe("You're all caught up");
    expect(fmt("It''s {n}", { n: 1 })).toBe("It's 1");
    expect(fmt("Use '{name}' to name it")).toBe('Use {name} to name it');
    expect(fmt("{n, plural, other {'#' is #}}", { n: 2 })).toBe('# is 2');
  });

  it('treats # outside a plural as text', () => {
    expect(fmt('Issue #12')).toBe('Issue #12');
  });

  it('throws on a missing or wrong value instead of showing a broken message', () => {
    expect(() => fmt('Hi {name}')).toThrow('missing value for "name"');
    expect(() => fmt('{n, plural, other {#}}', { n: 'many' })).toThrow('must be a number');
    expect(() => fmt('{n, plural, other {#}}', { n: null })).toThrow('must be a number');
    expect(() => fmt('{d, date}', { d: 'not a date' })).toThrow('must be a date');
  });

  it('throws on a message that is not valid ICU', () => {
    expect(() => parseMessage('Hi {name')).toThrow();
    expect(() => parseMessage('Hi }')).toThrow('unexpected "}"');
    expect(() => parseMessage('{n, plural, one {x}}')).toThrow('"other"');
    expect(() => parseMessage('{n, unknown, other {x}}')).toThrow('unsupported format');
    expect(() => parseMessage('{, plural, other {x}}')).toThrow('a name was expected');
  });

  it('does not use eval or the Function constructor', () => {
    // The policy forbids both (MOH-170): the whole point of this module.
    const original = globalThis.Function;
    const guard = vi.fn(() => {
      throw new Error('Function constructor used');
    });
    (globalThis as { Function: unknown }).Function = guard;
    try {
      expect(fmt('{n, plural, one {# thing} other {# things}}', { n: 3 })).toBe('3 things');
    } finally {
      (globalThis as { Function: unknown }).Function = original;
    }
    expect(guard).not.toHaveBeenCalled();
  });
});
