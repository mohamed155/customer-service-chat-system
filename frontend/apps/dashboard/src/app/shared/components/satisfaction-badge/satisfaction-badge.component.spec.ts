import { TestBed } from '@angular/core/testing';
import { SatisfactionBadgeComponent } from './satisfaction-badge.component';

describe('SatisfactionBadgeComponent', () => {
  function createComponent(rating: number) {
    TestBed.configureTestingModule({
      imports: [SatisfactionBadgeComponent],
    });
    const fixture = TestBed.createComponent(SatisfactionBadgeComponent);
    fixture.componentRef.setInput('rating', rating);
    fixture.detectChanges();
    return fixture;
  }

  it('renders rating 5 with green tone and accessible label', () => {
    const fixture = createComponent(5);
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('★ 5');
    expect(el.classList.contains('green')).toBe(true);
    expect(el.getAttribute('aria-label')).toBe('Rated 5 out of 5');
  });

  it('renders rating 4 with green tone and accessible label', () => {
    const fixture = createComponent(4);
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('★ 4');
    expect(el.classList.contains('green')).toBe(true);
    expect(el.getAttribute('aria-label')).toBe('Rated 4 out of 5');
  });

  it('renders rating 3 with amber tone and accessible label', () => {
    const fixture = createComponent(3);
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('★ 3');
    expect(el.classList.contains('amber')).toBe(true);
    expect(el.getAttribute('aria-label')).toBe('Rated 3 out of 5');
  });

  it('renders rating 2 with red tone and accessible label', () => {
    const fixture = createComponent(2);
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('★ 2');
    expect(el.classList.contains('red')).toBe(true);
    expect(el.getAttribute('aria-label')).toBe('Rated 2 out of 5');
  });

  it('renders rating 1 with red tone and accessible label', () => {
    const fixture = createComponent(1);
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('★ 1');
    expect(el.classList.contains('red')).toBe(true);
    expect(el.getAttribute('aria-label')).toBe('Rated 1 out of 5');
  });
});
