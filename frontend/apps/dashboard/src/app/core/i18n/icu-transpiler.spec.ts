import { TestBed } from '@angular/core/testing';
import { IcuTranspiler } from './icu-transpiler';

describe('IcuTranspiler', () => {
  const transpiler = () => TestBed.inject(IcuTranspiler);
  const transpile = (value: string, params: Record<string, unknown> = {}) =>
    transpiler().transpile({ value, params, translation: {}, key: 'k' });

  beforeEach(() => TestBed.configureTestingModule({ providers: [IcuTranspiler] }));

  it('returns a message without braces or quotes untouched', () => {
    expect(transpile('Plain text')).toBe('Plain text');
  });

  it('formats ICU messages and keeps Transloco’s own {{ }} interpolation', () => {
    expect(transpile('{n, plural, one {# file} other {# files}}', { n: 2 })).toBe('2 files');
    expect(transpile('Hi {{ name }}', { name: 'Ada' })).toBe('Hi Ada');
  });

  it('uses the plural rules of the active language', () => {
    transpiler().onLangChanged('pl');
    expect(transpile('{n, plural, few {few} other {other}}', { n: 3 })).toBe('few');
  });

  it('is loud about a broken message in development and tests', () => {
    expect(() => transpile('{n, plural, one {x}}', { n: 1 })).toThrow('Bad message "k"');
    expect(() => transpile('Hi {name}')).toThrow('Bad message "k"');
  });
});
