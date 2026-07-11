import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { LoadingStateComponent } from './loading-state.component';

describe('LoadingStateComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [LoadingStateComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('renders the default label and spinner element', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(LoadingStateComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).toContain('Loading');
    expect(element.querySelector('span[aria-hidden="true"]')).toBeTruthy();
  });

  it('renders a custom label when provided', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(LoadingStateComponent);
    fixture.componentRef.setInput('label', 'Fetching analytics');
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Fetching analytics');
  });
});
