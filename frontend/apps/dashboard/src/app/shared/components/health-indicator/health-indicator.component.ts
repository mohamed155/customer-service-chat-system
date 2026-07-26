import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-health-indicator',
  standalone: true,
  imports: [TuiIcon],
  template: `
    <span class="indicator" [class]="colorClass()">
      <tui-icon [icon]="iconName()" />
      <span>{{ label() }}</span>
    </span>
  `,
  styles: [
    `
      .indicator {
        display: inline-flex;
        align-items: center;
        gap: var(--app-space-1);
        font-weight: 600;
        font-size: var(--app-font-sm);
      }
      .healthy {
        color: var(--app-green);
      }
      .degraded {
        color: var(--app-amber);
      }
      .unhealthy {
        color: var(--app-red);
      }
      .healthy tui-icon {
        color: var(--app-green);
      }
      .degraded tui-icon {
        color: var(--app-amber);
      }
      .unhealthy tui-icon {
        color: var(--app-red);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HealthIndicatorComponent {
  readonly verdict = input.required<'healthy' | 'degraded' | 'unhealthy'>();

  protected readonly label = computed(() => {
    switch (this.verdict()) {
      case 'healthy':
        return 'Healthy';
      case 'degraded':
        return 'Degraded';
      case 'unhealthy':
        return 'Unhealthy';
    }
  });

  protected readonly colorClass = computed(() => this.verdict());

  protected readonly iconName = computed(() => {
    switch (this.verdict()) {
      case 'healthy':
        return '@tui.circle-check';
      case 'degraded':
        return '@tui.triangle-alert';
      case 'unhealthy':
        return '@tui.circle-x';
    }
  });
}
