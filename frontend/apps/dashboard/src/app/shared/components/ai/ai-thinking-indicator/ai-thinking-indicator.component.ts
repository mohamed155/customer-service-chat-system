import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-ai-thinking-indicator',
  templateUrl: './ai-thinking-indicator.component.html',
  styleUrl: './ai-thinking-indicator.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AiThinkingIndicatorComponent {}
