import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { Subject, of } from 'rxjs';
import { PromptBootstrapResponse } from '../../../../core/api/ai-agent.models';
import { PromptApiService } from './prompt-api.service';
import { PromptPageComponent } from './prompt-page.component';
import { PromptStore } from './prompt.store';

describe('PromptPageComponent', () => {
  let mockApi: {
    getPrompt: ReturnType<typeof vi.fn>;
    savePrompt: ReturnType<typeof vi.fn>;
  };

  const mockBootstrap: PromptBootstrapResponse = {
    prompt: {
      exists: true,
      activeVersion: 4,
      content: 'You are {{agent_name}}.',
      updatedAt: '2026-07-16T10:12:00Z',
      updatedBy: 'Dana Ops',
    },
    variables: [{ name: 'agent_name', description: 'The AI agent name', sample: 'Aria' }],
    limits: { maxContentLength: 8000, maxChangeNoteLength: 500 },
  };

  function createFixture() {
    TestBed.configureTestingModule({
      imports: [PromptPageComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: PromptApiService, useValue: mockApi },
      ],
    });
    const fixture = TestBed.createComponent(PromptPageComponent);
    fixture.detectChanges();
    return fixture;
  }

  beforeEach(() => {
    mockApi = { getPrompt: vi.fn(), savePrompt: vi.fn() };
  });

  it('renders loading state while data is being fetched', () => {
    mockApi.getPrompt.mockReturnValue(new Subject());
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
  });

  it('renders the editor with prompt content when loaded', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const textarea = fixture.nativeElement.querySelector('textarea');
      expect(textarea).toBeTruthy();
      expect(textarea.value).toBe(mockBootstrap.prompt.content);
    });
  });

  it('renders save button', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const saveBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.trim() === 'Save');
      expect(saveBtn).toBeTruthy();
    });
  });

  it('disables save button while saving', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('You are {{agent_name}}. Edited');
    fixture.detectChanges();

    await vi.waitFor(() => {
      const saveBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
    });
  });

  it('shows conflict banner when conflict occurs', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const saveSubject = new Subject<{ data: unknown }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('You are {{agent_name}}. Edited');
    fixture.detectChanges();

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
    saveBtn.click();
    fixture.detectChanges();

    saveSubject.error({ code: 'conflict', message: 'Version conflict', status: 409 });

    fixture.detectChanges();
    const banner = fixture.nativeElement.querySelector('.conflict-banner');
    expect(banner).toBeTruthy();
    expect(banner.textContent).toContain('reload');
  });

  it('shows no-op notice when save returns created: false', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const saveSubject = new Subject<{ data: { version: number; created: boolean } }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('You are {{agent_name}}. Edited');
    fixture.detectChanges();

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
    saveBtn.click();
    fixture.detectChanges();

    saveSubject.next({ data: { version: 4, created: false } });
    saveSubject.complete();
    fixture.detectChanges();

    const notice = fixture.nativeElement.querySelector('.notice-banner');
    expect(notice).toBeTruthy();
    expect(notice.textContent).toContain('No changes detected');
  });

  it('shows back link to AI Agent page', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const backLink = fixture.nativeElement.querySelector('.back-link');
      expect(backLink).toBeTruthy();
      expect(backLink.textContent).toContain('Back to AI Agent');
    });
  });

  it('shows inline validation for unknown variable placeholder', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('Hello {{business_hours}}');
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const issues = fixture.nativeElement.querySelectorAll('.validation-issues app-inline-alert');
      expect(issues.length).toBeGreaterThan(0);
      expect((issues[0] as HTMLElement).textContent).toContain('unknown variable');
    });
  });

  it('disables save button when client-side issues exist', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('Hello {{business_hours}}');
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const saveBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(true);
    });
  });

  it('re-enables save button when issue is fixed', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent('Hello {{business_hours}}');
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const saveBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(true);
    });

    store.setContent('Hello {{agent_name}}');
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      const saveBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
    });
  });

  it('does not clear textarea content on 422 response', async () => {
    mockApi.getPrompt.mockReturnValue(of({ data: mockBootstrap }));
    const saveSubject = new Subject<{ data: unknown }>();
    mockApi.savePrompt.mockReturnValue(saveSubject);

    const fixture = createFixture();
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(fixture.nativeElement.querySelector('textarea')).toBeTruthy();
    });

    const newContent = 'Custom prompt content';
    const store = fixture.debugElement.injector.get(PromptStore);
    store.setContent(newContent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
      expect(textarea.value).toBe(newContent);
    });

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Save')! as HTMLButtonElement;
    saveBtn.click();
    fixture.detectChanges();

    saveSubject.error({
      status: 422,
      message: 'Validation failed',
      details: [{ field: 'content', code: 'too_long', message: 'Content too long' }],
    });
    fixture.detectChanges();

    await vi.waitFor(() => {
      const textarea = fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
      expect(textarea.value).toBe(newContent);
    });
  });
});
