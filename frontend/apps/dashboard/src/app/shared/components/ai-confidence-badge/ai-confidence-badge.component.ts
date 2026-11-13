import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { TranslocoPipe } from '@jsverse/transloco';
import { marker } from '../../../core/i18n/marker';

export type ConfidenceBand = 'high' | 'medium' | 'low';

@Component({
  imports: [TranslocoPipe],
  selector: 'app-ai-confidence-badge',
  templateUrl: './ai-confidence-badge.component.html',
  styleUrl: './ai-confidence-badge.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiConfidenceBadgeComponent {
  readonly band = input.required<ConfidenceBand>();

  protected readonly label = computed(() => {
    switch (this.band()) {
      case 'high':
        return marker('confidence.high');
      case 'medium':
        return marker('confidence.medium');
      case 'low':
        return marker('confidence.low');
    }
  });
}
