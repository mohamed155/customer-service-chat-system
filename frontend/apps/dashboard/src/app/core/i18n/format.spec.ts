import { TestBed } from '@angular/core/testing';
import { LocaleDatePipe, LocaleNumberPipe, formatDate, formatNumber } from './format';
import { LocaleService } from './locale.service';
import { RelativeTimePipe } from './relative-time.pipe';

describe('formatDate and formatNumber', () => {
  const date = new Date('2026-06-16T14:30:00');

  it('write a date the way the locale does', () => {
    expect(formatDate(date, 'en', 'date')).toBe('Jun 16, 2026');
    expect(formatDate(date, 'de', 'date')).toBe('16.06.2026');
    expect(formatDate(date, 'en', 'longDate')).toBe('Tuesday, June 16');
    expect(formatDate(date, 'en', 'shortDate')).toBe('Jun 16');
    expect(formatDate(date, 'en', 'dateTime')).toMatch(/Jun 16, 2026.*2:30/);
  });

  it('accept an ISO string and a timestamp', () => {
    expect(formatDate('2026-06-16T14:30:00', 'en', 'date')).toBe('Jun 16, 2026');
    expect(formatDate(date.getTime(), 'en', 'date')).toBe('Jun 16, 2026');
  });

  it('give an empty text for no value or a bad one', () => {
    expect(formatDate(null, 'en')).toBe('');
    expect(formatDate(undefined, 'en')).toBe('');
    expect(formatDate('', 'en')).toBe('');
    expect(formatDate('not a date', 'en')).toBe('');
  });

  it('write a number the way the locale does', () => {
    expect(formatNumber(1234567.5, 'en')).toBe('1,234,567.5');
    expect(formatNumber(1234567.5, 'de')).toBe('1.234.567,5');
    expect(formatNumber(0.5, 'en', { style: 'percent' })).toBe('50%');
    expect(formatNumber(null, 'en')).toBe('');
    expect(formatNumber(Number.NaN, 'en')).toBe('');
  });
});

describe('the localeDate, localeNumber and relativeTime pipes', () => {
  it('follow the active locale', () => {
    const locale = TestBed.inject(LocaleService);
    const date = TestBed.runInInjectionContext(() => new LocaleDatePipe());
    const number = TestBed.runInInjectionContext(() => new LocaleNumberPipe());
    expect(date.transform('2026-06-16T14:30:00', 'date')).toBe('Jun 16, 2026');
    expect(number.transform(1234.5)).toBe('1,234.5');
    vi.spyOn(locale, 'formatLocale').mockReturnValue('de');
    expect(date.transform('2026-06-16T14:30:00', 'date')).toBe('16.06.2026');
    expect(number.transform(1234.5)).toBe('1.234,5');
  });

  it('relativeTime says how long ago, and the date from a week on', () => {
    const pipe = TestBed.runInInjectionContext(() => new RelativeTimePipe());
    const now = new Date('2026-06-16T12:00:00').getTime();
    const ago = (ms: number) => new Date(now - ms).toISOString();
    expect(pipe.transform(ago(30_000), now)).toBe('just now');
    expect(pipe.transform(ago(5 * 60_000), now)).toBe('5m ago');
    expect(pipe.transform(ago(3 * 3_600_000), now)).toBe('3h ago');
    expect(pipe.transform(ago(2 * 86_400_000), now)).toBe('2d ago');
    expect(pipe.transform(ago(8 * 86_400_000), now)).toBe('Jun 8');
    expect(pipe.transform(null)).toBe('');
    expect(pipe.transform('nonsense')).toBe('');
  });
});
