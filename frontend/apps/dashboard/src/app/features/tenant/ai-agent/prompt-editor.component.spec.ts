import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { PromptEditorComponent } from './prompt-editor.component';

describe('PromptEditorComponent', () => {
  function createComponent(maxLength?: number, initialValue?: string) {
    TestBed.configureTestingModule({
      imports: [PromptEditorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(PromptEditorComponent);
    if (maxLength !== undefined) fixture.componentRef.setInput('maxLength', maxLength);
    if (initialValue !== undefined) fixture.componentRef.setInput('value', initialValue);
    fixture.detectChanges();
    return fixture;
  }

  it('renders the textarea with a character counter', () => {
    const fixture = createComponent();
    const textarea = fixture.nativeElement.querySelector('textarea');
    expect(textarea).toBeTruthy();
    expect(fixture.nativeElement.textContent).toMatch(/\d+ \/ 8000/);
  });

  it('updates counter when value changes', () => {
    const fixture = createComponent();
    const textarea = fixture.nativeElement.querySelector('textarea');
    textarea.value = 'hello';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('5 / 8000');
  });

  it('shows warning styling when near the character limit', () => {
    const fixture = createComponent(10, '123456789');
    fixture.detectChanges();
    const counter = fixture.nativeElement.querySelector('.counter');
    expect(counter.classList.contains('warning')).toBe(true);
  });
});
