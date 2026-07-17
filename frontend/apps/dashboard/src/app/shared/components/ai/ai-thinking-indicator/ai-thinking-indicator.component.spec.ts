import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { AiThinkingIndicatorComponent } from './ai-thinking-indicator.component';

describe('AiThinkingIndicatorComponent', () => {
  function createComponent() {
    TestBed.configureTestingModule({
      imports: [AiThinkingIndicatorComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(AiThinkingIndicatorComponent);
    fixture.detectChanges();
    return fixture;
  }

  it('renders the host element', () => {
    const fixture = createComponent();
    const el = fixture.nativeElement as HTMLElement;
    expect(el).toBeTruthy();
  });

  it('renders the AI is thinking label', () => {
    const fixture = createComponent();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('AI is thinking...');
  });

  it('renders three animated dots', () => {
    const fixture = createComponent();
    const el = fixture.nativeElement as HTMLElement;
    const dots = el.querySelectorAll('.dot');
    expect(dots.length).toBe(3);
  });
});
