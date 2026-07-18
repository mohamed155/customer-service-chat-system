import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Observable, of, throwError } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { ConversationsApiService } from './conversations-api.service';
import { ConversationSummaryComponent } from './conversation-summary.component';

describe('ConversationSummaryComponent', () => {
  let api: { requestSummary: ReturnType<typeof vi.fn> };

  function configureTesting() {
    api = { requestSummary: vi.fn() };
    TestBed.configureTestingModule({
      imports: [ConversationSummaryComponent],
      providers: [
        provideZonelessChangeDetection(),
        { provide: ConversationsApiService, useValue: api },
      ],
    });
  }

  async function createComponent(conversationId: string) {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationSummaryComponent);
    fixture.componentRef.setInput('conversationId', conversationId);
    fixture.detectChanges();
    return { fixture };
  }

  it('renders summarize button in initial state', async () => {
    configureTesting();
    api.requestSummary.mockReturnValue(
      of({ data: { summary: '', generatedAt: '', messageCount: 0 } }),
    );
    const { fixture } = await createComponent('c1');
    expect(fixture.nativeElement.textContent).toContain('Summarize');
  });

  it('shows loading state while requesting summary', async () => {
    configureTesting();
    api.requestSummary.mockReturnValue(new Observable(() => {}));
    const { fixture } = await createComponent('c1');
    fixture.nativeElement.querySelector('button').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Generating…');
  });

  it('renders summary text on success', async () => {
    configureTesting();
    api.requestSummary.mockReturnValue(
      of({
        data: {
          summary: 'The customer wants a refund for order #1234.',
          generatedAt: '2026-07-18T10:00:00Z',
          messageCount: 23,
        },
      }),
    );
    const { fixture } = await createComponent('c1');
    fixture.nativeElement.querySelector('button').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain(
      'The customer wants a refund for order #1234.',
    );
  });

  it('shows inline error on API failure and thread stays usable', async () => {
    configureTesting();
    const apiError: ApiError = {
      code: 'provider_failure',
      message: 'AI provider unavailable',
      status: 502,
    };
    api.requestSummary.mockReturnValue(throwError(() => apiError));
    const { fixture } = await createComponent('c1');
    fixture.nativeElement.querySelector('button').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('AI provider unavailable');
    expect(fixture.nativeElement.textContent).toContain('Summarize');
  });

  it('shows inline error with fallback text when error has no message', async () => {
    configureTesting();
    api.requestSummary.mockReturnValue(throwError(() => ({})));
    const { fixture } = await createComponent('c1');
    fixture.nativeElement.querySelector('button').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Failed to generate summary');
  });
});
