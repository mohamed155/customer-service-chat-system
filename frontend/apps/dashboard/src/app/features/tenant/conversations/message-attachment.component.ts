import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { formatNumber } from '../../../core/i18n/format';
import { LocaleService } from '../../../core/i18n/locale.service';
import { marker } from '../../../core/i18n/marker';
import { MessageAttachment } from '../../../core/api/tenant-api.models';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';

@Component({
  providers: [provideTranslocoScope('conversations')],
  imports: [TranslocoPipe],
  selector: 'app-message-attachment',
  standalone: true,
  templateUrl: './message-attachment.component.html',
  styleUrl: './message-attachment.component.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MessageAttachmentComponent {
  readonly attachment = input.required<MessageAttachment>();
  private readonly locale = inject(LocaleService);

  /** The catalog key of a file size and the values it needs. */
  protected sizeText(bytes: number): { key: string; params: { size: string } } {
    const locale = this.locale.formatLocale();
    const one = { minimumFractionDigits: 1, maximumFractionDigits: 1 };
    if (bytes < 1024) {
      return {
        key: marker('conversations.attachment.bytes'),
        params: { size: formatNumber(bytes, locale) },
      };
    }
    if (bytes < 1024 * 1024) {
      return {
        key: marker('conversations.attachment.kilobytes'),
        params: { size: formatNumber(bytes / 1024, locale, one) },
      };
    }
    return {
      key: marker('conversations.attachment.megabytes'),
      params: { size: formatNumber(bytes / (1024 * 1024), locale, one) },
    };
  }
}
