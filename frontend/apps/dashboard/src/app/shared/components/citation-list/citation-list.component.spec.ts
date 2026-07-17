import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { Citation } from '../../../core/api/tenant-api.models';
import { CitationListComponent } from './citation-list.component';

describe('CitationListComponent', () => {
  const availableCitation: Citation = {
    knowledgeItemId: 'kb-1',
    itemTitle: 'Returns and exchanges policy',
    passageText: 'Customers may return items within 30 days...',
    relevanceScore: 0.95,
    itemAvailable: true,
  };

  const unavailableCitation: Citation = {
    knowledgeItemId: 'kb-2',
    itemTitle: 'Old warranty policy',
    passageText: 'This policy was superseded...',
    relevanceScore: 0.82,
    itemAvailable: false,
  };

  function createComponent(citations: Citation[]) {
    TestBed.configureTestingModule({
      imports: [CitationListComponent],
      providers: [provideRouter([]), provideTaiga(), provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(CitationListComponent);
    fixture.componentRef.setInput('citations', citations);
    fixture.detectChanges();
    return fixture;
  }

  it('renders nothing when citations array is empty', () => {
    const fixture = createComponent([]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent?.trim()).toBe('');
  });

  it('renders a link for an available citation', () => {
    const fixture = createComponent([availableCitation]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Returns and exchanges policy');
    expect(el.querySelector('a')).toBeTruthy();
    expect(el.querySelector('tui-icon')).toBeTruthy();
  });

  it('renders an unavailable badge for a deleted citation', () => {
    const fixture = createComponent([unavailableCitation]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Old warranty policy');
    expect(el.textContent).toContain('No longer available');
    expect(el.querySelector('a')).toBeFalsy();
    expect(el.querySelector('.unavailable')).toBeTruthy();
  });

  it('renders mixed states when both available and unavailable citations exist', () => {
    const fixture = createComponent([availableCitation, unavailableCitation]);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Returns and exchanges policy');
    expect(el.textContent).toContain('Old warranty policy');
    expect(el.textContent).toContain('No longer available');
    expect(el.querySelectorAll('.citation-chip').length).toBe(2);
    expect(el.querySelectorAll('a').length).toBe(1);
    expect(el.querySelectorAll('.unavailable').length).toBe(1);
  });
});
