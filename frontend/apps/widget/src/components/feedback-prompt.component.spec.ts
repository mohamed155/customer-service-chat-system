import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { FeedbackPromptComponent } from './feedback-prompt.component';

describe('FeedbackPromptComponent', () => {
  let fixture: ComponentFixture<FeedbackPromptComponent>;
  let component: FeedbackPromptComponent;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [FeedbackPromptComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(FeedbackPromptComponent);
    component = fixture.componentInstance;
    fixture.componentRef.setInput('state', 'prompt');
    fixture.detectChanges();
  });

  it('does not submit on star click, only sets selected rating', () => {
    const spy = vi.spyOn(component.submitFeedback, 'emit');

    component.onRate(3);
    fixture.detectChanges();

    expect(spy).not.toHaveBeenCalled();
    expect(component.selectedRating()).toBe(3);
  });

  it('shows submit button after rating is selected', () => {
    expect(fixture.debugElement.query(By.css('.submit-btn'))).toBeNull();

    component.selectedRating.set(4);
    fixture.detectChanges();

    expect(fixture.debugElement.query(By.css('.submit-btn'))).not.toBeNull();
  });

  it('submits rating and comment on Send feedback click', () => {
    const spy = vi.spyOn(component.submitFeedback, 'emit');
    component.selectedRating.set(5);
    component.comment.set('Great service!');
    fixture.detectChanges();

    const btn = fixture.debugElement.query(By.css('.submit-btn'));
    btn.nativeElement.click();

    expect(spy).toHaveBeenCalledWith({ rating: 5, comment: 'Great service!' });
    expect(component.selectedRating()).toBe(0);
    expect(component.comment()).toBe('');
  });

  it('submits without comment when comment is empty', () => {
    const spy = vi.spyOn(component.submitFeedback, 'emit');
    component.selectedRating.set(2);
    component.comment.set('');
    fixture.detectChanges();

    const btn = fixture.debugElement.query(By.css('.submit-btn'));
    btn.nativeElement.click();

    expect(spy).toHaveBeenCalledWith({ rating: 2, comment: undefined });
  });

  it('disables submit button when comment exceeds 2000 chars', () => {
    component.selectedRating.set(3);
    component.comment.set('a'.repeat(2001));
    fixture.detectChanges();

    const btn = fixture.debugElement.query(By.css('.submit-btn'));
    expect(btn.nativeElement.disabled).toBe(true);
  });

  it('does not show submit button when no rating selected', () => {
    expect(component.selectedRating()).toBe(0);
    fixture.detectChanges();
    expect(fixture.debugElement.query(By.css('.submit-btn'))).toBeNull();
  });

  it('saves textarea value to comment signal on input', () => {
    const textarea = fixture.debugElement.query(By.css('.comment-input'));
    expect(textarea).not.toBeNull();

    const el = textarea.nativeElement;
    el.value = 'some feedback';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect(component.comment()).toBe('some feedback');
  });
});
