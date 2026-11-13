import { baseLanguage, candidateLocales, formattingLocale, parseLocale } from './locale';

describe('locale helpers', () => {
  it('accepts well-formed language tags and canonicalises them', () => {
    expect(parseLocale('en')).toBe('en');
    expect(parseLocale('de-at')).toBe('de-AT');
    expect(parseLocale('zh-Hant')).toBe('zh-Hant');
  });

  it('rejects anything that is not a plain language tag', () => {
    for (const bad of [
      '',
      null,
      undefined,
      42,
      '../etc/passwd',
      'en/../../x',
      'de_DE',
      'en-u-ca-x',
      'x'.repeat(40),
      'fr?x=1',
    ]) {
      expect(parseLocale(bad as unknown), String(bad)).toBeNull();
    }
  });

  it('gives the base language', () => {
    expect(baseLanguage('de-AT')).toBe('de');
    expect(baseLanguage('EN')).toBe('en');
  });

  it('orders candidates: the stored choice, then the browser, tag before base language', () => {
    expect(candidateLocales('fr-CA', ['de-AT', 'it'])).toEqual([
      'fr-CA',
      'fr',
      'de-AT',
      'de',
      'it',
      'en',
    ]);
  });

  it('stops at the first English tag: English is bundled, so nothing after it is reached', () => {
    expect(candidateLocales(null, ['en-GB', 'de'])).toEqual(['en']);
    expect(candidateLocales('de', ['en-US', 'fr'])).toEqual(['de', 'en']);
  });

  it('always ends with English and skips invalid and repeated tags', () => {
    expect(candidateLocales(null, [])).toEqual(['en']);
    expect(candidateLocales('nonsense!!', ['de', 'de-DE', 'de'])).toEqual(['de', 'de-DE', 'en']);
  });

  it('limits how many files are tried', () => {
    const many = ['aa', 'bb', 'cc', 'dd', 'ee', 'ff', 'gg', 'hh'];
    expect(candidateLocales(null, many).length).toBeLessThanOrEqual(7);
  });

  it('writes numbers and dates in the tag the person asked for when the language matches', () => {
    expect(formattingLocale('en', null, ['en-GB'])).toBe('en-GB');
    expect(formattingLocale('de', 'de-AT', ['de-DE'])).toBe('de-AT');
    expect(formattingLocale('de', null, ['fr-FR'])).toBe('de');
    expect(formattingLocale('en', null, [])).toBe('en');
  });
});
