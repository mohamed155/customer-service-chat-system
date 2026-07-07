import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { KnowledgeBaseComponent } from './knowledge-base.component';

describe('KnowledgeBaseComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [KnowledgeBaseComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('shows an empty state when the search has no matches', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(KnowledgeBaseComponent);
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="search"]') as HTMLInputElement;

    input.value = 'zzzz no match';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).toContain('No articles match');
  });
});
