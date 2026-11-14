import { Pipe, PipeTransform, inject } from '@angular/core';
import { LocaleService } from './locale.service';

/** The ways a date is written. The words and the order come from the locale, never from here. */
export type DateStyle =
  'date' | 'shortDate' | 'dateTime' | 'time' | 'longDate' | 'monthYear' | 'utcDate' | 'utcDateTime';

const DATE_OPTIONS: Readonly<Record<DateStyle, Intl.DateTimeFormatOptions>> = {
  date: { dateStyle: 'medium' },
  shortDate: { month: 'short', day: 'numeric' },
  dateTime: { dateStyle: 'medium', timeStyle: 'short' },
  time: { timeStyle: 'short' },
  longDate: { weekday: 'long', month: 'long', day: 'numeric' },
  monthYear: { month: 'long', year: 'numeric' },
  // For a figure the API gives in UTC (a reporting window): written in UTC, with the zone named.
  utcDate: { dateStyle: 'medium', timeZone: 'UTC' },
  utcDateTime: {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZone: 'UTC',
    timeZoneName: 'short',
  },
};

const dateFormats = new Map<string, Intl.DateTimeFormat>();
const numberFormats = new Map<string, Intl.NumberFormat>();

/** A date, a timestamp or an ISO string, written for a locale. Empty for no value or a bad one. */
export function formatDate(
  value: Date | string | number | null | undefined,
  locale: string,
  style: DateStyle = 'dateTime',
): string {
  if (value === null || value === undefined || value === '') return '';
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const key = `${locale}|${style}`;
  let format = dateFormats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(locale, DATE_OPTIONS[style]);
    dateFormats.set(key, format);
  }
  return format.format(date);
}

/** A number written for a locale (`1,234.5` or `1.234,5`). Empty for no value. */
export function formatNumber(
  value: number | null | undefined,
  locale: string,
  options?: Intl.NumberFormatOptions,
): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '';
  const key = `${locale}|${JSON.stringify(options ?? {})}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale, options);
    numberFormats.set(key, format);
  }
  return format.format(value);
}

/**
 * `{{ createdAt | localeDate }}`, `{{ day | localeDate: 'date' }}`. Use it instead of `DatePipe`:
 * Angular's pipe needs locale data registered in code for every language, this one does not. It
 * reads the active locale, so it is impure and follows a language change.
 */
@Pipe({ name: 'localeDate', pure: false })
export class LocaleDatePipe implements PipeTransform {
  private readonly locale = inject(LocaleService);

  transform(value: Date | string | number | null | undefined, style: DateStyle = 'dateTime') {
    return formatDate(value, this.locale.formatLocale(), style);
  }
}

/** `{{ tokens | localeNumber }}`, `{{ rate | localeNumber: { style: 'percent' } }}`. */
@Pipe({ name: 'localeNumber', pure: false })
export class LocaleNumberPipe implements PipeTransform {
  private readonly locale = inject(LocaleService);

  transform(value: number | null | undefined, options?: Intl.NumberFormatOptions) {
    return formatNumber(value, this.locale.formatLocale(), options);
  }
}
