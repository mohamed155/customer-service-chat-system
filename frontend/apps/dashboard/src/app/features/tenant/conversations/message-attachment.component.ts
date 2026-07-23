import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { MessageAttachment } from '../../../core/api/tenant-api.models';

@Component({
  selector: 'app-message-attachment',
  standalone: true,
  templateUrl: './message-attachment.component.html',
  styleUrl: './message-attachment.component.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MessageAttachmentComponent {
  readonly attachment = input.required<MessageAttachment>();

  protected formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
}
