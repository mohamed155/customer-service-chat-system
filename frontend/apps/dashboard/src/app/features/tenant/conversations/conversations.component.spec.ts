import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { ConversationsComponent } from './conversations.component';

describe('ConversationsComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [ConversationsComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('updates the thread and customer panel when selecting a conversation', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    const items = element.querySelectorAll('.item');
    (items[1] as HTMLButtonElement).click();
    fixture.detectChanges();

    expect(element.textContent).toContain('Jon Bell');
    expect(element.textContent).toContain('This is the third failed pickup window');
  });

  it('filters the rendered list by status', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    (
      Array.from(element.querySelectorAll('.filters button')).find(
        (button) => button.textContent?.trim() === 'Open',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    expect(element.textContent).toContain('Can someone confirm the delivery address?');
    expect(element.textContent).not.toContain('The invoice copy is all I needed.');
  });

  it('does not add an extra header landmark inside the dashboard page content', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ConversationsComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelectorAll('header')).toHaveLength(0);
  });
});
