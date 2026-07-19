import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { SatisfactionSummaryComponent } from './satisfaction-summary.component';

describe('SatisfactionSummaryComponent', () => {
  async function createComponent(config?: {
    averageRating?: number | null;
    feedbackCount?: number;
  }) {
    await TestBed.configureTestingModule({
      imports: [SatisfactionSummaryComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();

    const fixture = TestBed.createComponent(SatisfactionSummaryComponent);
    const component = fixture.componentInstance;

    if (config) {
      if (config.averageRating !== undefined)
        fixture.componentRef.setInput('averageRating', config.averageRating);
      if (config.feedbackCount !== undefined)
        fixture.componentRef.setInput('feedbackCount', config.feedbackCount);
    }

    fixture.detectChanges();
    return { fixture, component };
  }

  it('shows average with 1 decimal and count caption when populated', async () => {
    const { fixture } = await createComponent({ averageRating: 4.256, feedbackCount: 10 });
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('4.3');
    expect(el.textContent).toContain('from 10 ratings');
  });

  it('rounds to 1 decimal place', async () => {
    const { fixture } = await createComponent({ averageRating: 3.04, feedbackCount: 5 });
    expect(fixture.nativeElement.textContent).toContain('3.0');
  });

  it('shows empty state when feedbackCount is 0', async () => {
    const { fixture } = await createComponent({ averageRating: null, feedbackCount: 0 });
    expect(fixture.nativeElement.textContent).toContain('No ratings yet');
  });

  it('shows empty state when feedbackCount is 0 even with a rating', async () => {
    const { fixture } = await createComponent({ averageRating: 4.5, feedbackCount: 0 });
    expect(fixture.nativeElement.textContent).toContain('No ratings yet');
  });

  it('shows singular "rating" when count is 1', async () => {
    const { fixture } = await createComponent({ averageRating: 5.0, feedbackCount: 1 });
    expect(fixture.nativeElement.textContent).toContain('from 1 rating');
  });
});
