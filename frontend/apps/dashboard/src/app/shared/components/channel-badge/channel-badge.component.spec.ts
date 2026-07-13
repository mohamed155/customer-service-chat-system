import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { ChannelBadgeComponent } from './channel-badge.component';

describe('ChannelBadgeComponent', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [ChannelBadgeComponent],
      providers: [provideTaiga(), provideZonelessChangeDetection()],
    });
  });

  it.each([
    ['email', 'Email'],
    ['phone', 'Phone'],
  ] as const)('renders the $1 identifier channel label and icon', (channel, label) => {
    const fixture = TestBed.createComponent(ChannelBadgeComponent);
    fixture.componentRef.setInput('channel', channel);
    fixture.detectChanges();

    const element: HTMLElement = fixture.nativeElement;
    expect(element.textContent).toContain(label);
    expect(element.querySelector('tui-icon')).toBeTruthy();
  });

  it('renders the web_chat identifier channel label', () => {
    const fixture = TestBed.createComponent(ChannelBadgeComponent);
    fixture.componentRef.setInput('channel', 'web_chat');
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Web chat');
  });
});
