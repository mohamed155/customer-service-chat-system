import { ChangeDetectionStrategy, Component, input } from '@angular/core';

@Component({
  selector: 'app-tool-result-viewer',
  imports: [],
  templateUrl: './tool-result-viewer.component.html',
  styleUrl: './tool-result-viewer.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ToolResultViewerComponent {
  readonly result = input<unknown>();
  readonly error = input<string>();

  protected expanded = false;

  protected toggle(): void {
    this.expanded = !this.expanded;
  }

  protected get hasContent(): boolean {
    return this.result() !== undefined || this.error() !== undefined;
  }

  protected prettyPrint(value: unknown): string {
    return JSON.stringify(value, null, 2);
  }
}
