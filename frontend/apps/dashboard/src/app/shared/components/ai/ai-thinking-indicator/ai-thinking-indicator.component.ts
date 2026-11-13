import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  imports: [TranslocoPipe],
  selector: 'app-ai-thinking-indicator',
  templateUrl: './ai-thinking-indicator.component.html',
  styleUrl: './ai-thinking-indicator.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiThinkingIndicatorComponent {}
