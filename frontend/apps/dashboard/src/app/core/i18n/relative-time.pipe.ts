import { Pipe, PipeTransform, inject } from '@angular/core';
import { TranslocoService } from '@jsverse/transloco';
import { formatDate } from './format';
import { LocaleService } from './locale.service';

/**
 * How long ago something happened, in the active language: "just now", "5m ago", "3h ago",
 * "2d ago", and from a week on the date ("Jun 16"). `{{ item.createdAt | relativeTime }}`. The
 * words are the `relativeTime.*` messages, so a translator owns the wording and the plural forms.
 */
@Pipe({ name: 'relativeTime', pure: false })
export class RelativeTimePipe implements PipeTransform {
  private readonly transloco = inject(TranslocoService);
  private readonly locale = inject(LocaleService);

  transform(value: string | number | Date | null | undefined, now: number = Date.now()): string {
    if (value === null || value === undefined || value === '') return '';
    const then = new Date(value).getTime();
    if (Number.isNaN(then)) return '';
    const seconds = Math.floor((now - then) / 1000);
    if (seconds < 60) return this.transloco.translate('relativeTime.justNow');
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return this.transloco.translate('relativeTime.minutes', { n: minutes });
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return this.transloco.translate('relativeTime.hours', { n: hours });
    const days = Math.floor(hours / 24);
    if (days < 7) return this.transloco.translate('relativeTime.days', { n: days });
    return formatDate(then, this.locale.formatLocale(), 'shortDate');
  }
}
