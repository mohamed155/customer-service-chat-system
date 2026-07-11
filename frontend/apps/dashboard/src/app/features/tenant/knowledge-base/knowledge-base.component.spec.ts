import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { KNOWLEDGE_FIXTURES } from '../../../shared/fixtures/knowledge.fixtures';
import { RoutedPageDataService } from '../routed-page-data.service';
import { KnowledgeBaseComponent } from './knowledge-base.component';

describe('KnowledgeBaseComponent', () => {
  const loadKnowledge = vi.fn();

  beforeEach(() => {
    loadKnowledge.mockReset();
    TestBed.configureTestingModule({
      imports: [KnowledgeBaseComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadKnowledge } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadKnowledge.mockReturnValue(of({ page: 'knowledge-base', data: KNOWLEDGE_FIXTURES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(KnowledgeBaseComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Returns and exchanges policy');
    });
  });

  it('shows an empty state when the search has no matches', async () => {
    loadKnowledge.mockReturnValue(of({ page: 'knowledge-base', data: KNOWLEDGE_FIXTURES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(KnowledgeBaseComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Returns and exchanges policy');
    });

    const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
    input.value = 'zzzznonexistent';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
  });

  it('moves from pending to empty state', async () => {
    loadKnowledge.mockReturnValue(of({ page: 'knowledge-base', data: [] }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(KnowledgeBaseComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from pending to error and retries', async () => {
    loadKnowledge.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(KnowledgeBaseComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadKnowledge.mockReturnValue(of({ page: 'knowledge-base', data: KNOWLEDGE_FIXTURES }));
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
