import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { PagePayload, RoutedPageDataService } from '../routed-page-data.service';
import { AiAgentComponent } from './ai-agent.component';

describe('AiAgentComponent', () => {
  const loadAiAgent = vi.fn();
  const MOCK_AGENT: PagePayload = {
    page: 'ai-agent',
    data: {
      allowedTopics: ['Shipping'],
      blockedTopics: ['Legal'],
      escalationRules: ['Escalate on angry sentiment'],
      timelineSteps: [{ label: 'Classify', detail: 'Support request' }],
    },
  };

  beforeEach(() => {
    loadAiAgent.mockReset();
    TestBed.configureTestingModule({
      imports: [AiAgentComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadAiAgent } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadAiAgent.mockReturnValue(of(MOCK_AGENT));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Behavior guardrails');
      expect(fixture.nativeElement.textContent).toContain('Shipping');
      expect(fixture.nativeElement.textContent).toContain('Legal');
    });
  });

  it('switches visible sections when tabs change', async () => {
    loadAiAgent.mockReturnValue(of(MOCK_AGENT));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Behavior guardrails');
    });

    const escalationTab = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Escalation')!;
    escalationTab.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Escalation triggers');
      expect(fixture.nativeElement.textContent).toContain('Escalate on angry sentiment');
    });
  });

  it('moves from pending to empty state', async () => {
    loadAiAgent.mockReturnValue(of(null));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from pending to error and retries', async () => {
    loadAiAgent.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AiAgentComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadAiAgent.mockReturnValue(of(MOCK_AGENT));
    const retryBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Try again')!;
    retryBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });
});
