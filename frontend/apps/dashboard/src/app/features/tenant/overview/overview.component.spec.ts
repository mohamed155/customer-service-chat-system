import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { OverviewComponent } from './overview.component';

describe('OverviewComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [OverviewComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('renders five metric cards and the main overview sections', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-metric-card').length).toBe(5);
    expect(element.textContent).toContain('Conversation trends');
    expect(element.textContent).toContain('Channel mix');
    expect(element.textContent).toContain('Recent activity');
  });

  it('removes the escalation alert when dismissed', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-escalation-banner')).toBeTruthy();
    (
      fixture.nativeElement.querySelector('[aria-label="Dismiss alert"]') as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-escalation-banner')).toBeFalsy();
  });
});
