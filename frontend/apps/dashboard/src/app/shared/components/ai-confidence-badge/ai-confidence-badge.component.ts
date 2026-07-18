import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

export type ConfidenceBand = 'high' | 'medium' | 'low';

@Component({
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
        return 'High';
      case 'medium':
        return 'Medium';
      case 'low':
        return 'Low';
    }
  });
}
