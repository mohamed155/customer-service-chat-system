import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { StarRatingComponent } from './star-rating.component';

describe('StarRatingComponent', () => {
  let component: StarRatingComponent;
  let fixture: ComponentFixture<StarRatingComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [StarRatingComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(StarRatingComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('emits 3 when clicking the 3rd star', () => {
    let emitted = 0;
    component.rate.subscribe((v) => (emitted = v));

    const stars = fixture.debugElement.queryAll(By.css('.star'));
    stars[2].nativeElement.click();

    expect(emitted).toBe(3);
  });

  it('does not emit when readonly', () => {
    fixture.componentRef.setInput('readonly', true);
    fixture.detectChanges();

    let emitted = 0;
    component.rate.subscribe((v) => (emitted = v));

    const stars = fixture.debugElement.queryAll(By.css('.star'));
    stars[2].nativeElement.click();

    expect(emitted).toBe(0);
  });

  it('each star has an accessible label', () => {
    const stars = fixture.debugElement.queryAll(By.css('.star'));
    expect(stars.length).toBe(5);

    stars.forEach((star, i) => {
      const label = star.nativeElement.getAttribute('aria-label');
      expect(label).toBe(`${i + 1} stars`);
    });
  });
});
