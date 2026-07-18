import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { ToolRequest } from '../../../core/api/tenant-api.models';
import { ToolResultViewerComponent } from '../tool-result-viewer/tool-result-viewer.component';

@Component({
  selector: 'app-tool-timeline-entry',
  imports: [ToolResultViewerComponent],
  templateUrl: './tool-timeline-entry.component.html',
  styleUrl: './tool-timeline-entry.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolTimelineEntryComponent {
  readonly request = input.required<ToolRequest>();

  protected argumentsExpanded = false;

  protected readonly hasArguments = computed(() => {
    const args = this.request().arguments;
    return args !== undefined && args !== null;
  });

  protected toggleArguments(): void {
    this.argumentsExpanded = !this.argumentsExpanded;
  }

  protected prettyPrint(value: unknown): string {
    return JSON.stringify(value, null, 2);
  }

  protected readonly isSuccess = computed(() => this.request().status === 'succeeded');

  protected readonly isError = computed(() => {
    const s = this.request().status;
    return (
      s === 'failed' ||
      s === 'timed_out' ||
      s === 'denied' ||
      s === 'expired' ||
      s === 'refused' ||
      s === 'cancelled'
    );
  });

  protected readonly statusChipVariant = computed(() => {
    switch (this.request().status) {
      case 'succeeded':
        return 'success';
      case 'failed':
      case 'timed_out':
      case 'denied':
      case 'cancelled':
        return 'error';
      case 'executing':
      case 'pending':
      case 'approved':
        return 'info';
      case 'awaiting_approval':
        return 'warning';
      case 'refused':
      case 'expired':
        return 'neutral';
      default:
        return 'neutral';
    }
  });

  protected readonly formattedDuration = computed(() => {
    const ms = this.request().durationMs;
    if (ms == null) return null;
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  });
}
