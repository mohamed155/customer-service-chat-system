import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { AiAgentComponent } from './ai-agent.component';

describe('AiAgentComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [AiAgentComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('switches visible sections when tabs change', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).toContain('Agent profile');
    (
      Array.from(element.querySelectorAll('[role="tab"]')).find(
        (tab) => tab.textContent?.trim() === 'Testing',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    expect(element.textContent).toContain('Test assistant');
    expect(element.textContent).toContain('Tool timeline');
  });
});
