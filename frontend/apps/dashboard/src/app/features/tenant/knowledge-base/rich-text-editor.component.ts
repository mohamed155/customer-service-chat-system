import { ChangeDetectionStrategy, Component, input, model } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TuiEditor } from '@taiga-ui/editor';
import { TuiEditorTool } from '@taiga-ui/editor/common';

@Component({
  selector: 'app-rich-text-editor',
  imports: [FormsModule, TuiEditor],
  templateUrl: './rich-text-editor.component.html',
  styles: [
    `
      tui-editor {
        display: block;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        min-height: 200px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RichTextEditorComponent {
  readonly value = model('');
  readonly placeholder = input('Write your content…');
  protected readonly tools = [
    TuiEditorTool.Bold,
    TuiEditorTool.Italic,
    TuiEditorTool.Size,
    TuiEditorTool.List,
    TuiEditorTool.Link,
  ] as const;
}
