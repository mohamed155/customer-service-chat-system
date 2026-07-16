import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ToneSelectorComponent } from './tone-selector.component';

describe('ToneSelectorComponent', () => {
  function createComponent(tones: string[]) {
    TestBed.configureTestingModule({
      imports: [ToneSelectorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ToneSelectorComponent);
    fixture.componentRef.setInput('tones', tones);
    fixture.detectChanges();
    return fixture;
  }

  it('renders a button for each tone with a capitalized label', () => {
    const fixture = createComponent(['calm', 'warm', 'playful']);
    const buttons = fixture.nativeElement.querySelectorAll('button');
    expect(buttons.length).toBe(3);
    expect(buttons[0].textContent?.trim()).toBe('Calm');
    expect(buttons[1].textContent?.trim()).toBe('Warm');
    expect(buttons[2].textContent?.trim()).toBe('Playful');
  });

  it('selects a tone on click', () => {
    const fixture = createComponent(['calm', 'warm']);
    const buttons = fixture.nativeElement.querySelectorAll('button');
    buttons[1].click();
    fixture.detectChanges();
    expect(buttons[1].getAttribute('aria-pressed')).toBe('true');
  });
});
