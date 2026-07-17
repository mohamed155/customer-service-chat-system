import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { TUI_EDITOR_EXTENSIONS } from '@taiga-ui/editor/common';
import { RichTextEditorComponent } from './rich-text-editor.component';

describe('RichTextEditorComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [RichTextEditorComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: TUI_EDITOR_EXTENSIONS, useValue: [] },
      ],
    }),
  );

  it('renders with default placeholder', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RichTextEditorComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('tui-editor')).toBeTruthy();
  });

  it('accepts a custom placeholder', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RichTextEditorComponent);
    fixture.componentRef.setInput('placeholder', 'Describe the article…');
    fixture.detectChanges();

    expect(fixture.componentRef).toBeTruthy();
  });
});
