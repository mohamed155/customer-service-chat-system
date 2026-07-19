import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { WidgetPreviewComponent } from './widget-preview.component';
import { WidgetInstance } from '../../../core/api/widget.models';

describe('WidgetPreviewComponent', () => {
  async function createFixture(formState: Partial<WidgetInstance> = {}) {
    await TestBed.configureTestingModule({
      imports: [WidgetPreviewComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();

    const fixture = TestBed.createComponent(WidgetPreviewComponent);
    fixture.componentRef.setInput('formState', formState);
    fixture.detectChanges();
    return fixture;
  }

  it('renders the preview container', async () => {
    const fixture = await createFixture();
    expect(fixture.nativeElement.querySelector('.preview-container')).toBeTruthy();
  });

  it('renders the phone preview element', async () => {
    const fixture = await createFixture();
    const phone = fixture.nativeElement.querySelector('.preview-phone');
    expect(phone).toBeTruthy();
  });

  it('sets --wgt-primary from formState', async () => {
    const fixture = await createFixture({ primaryColor: '#FF6600' });
    const phone = fixture.nativeElement.querySelector('.preview-phone') as HTMLElement;
    expect(phone.style.getPropertyValue('--wgt-primary')).toBe('#FF6600');
  });

  it('sets data-wgt-theme from formState', async () => {
    const fixture = await createFixture({ theme: 'dark' });
    const phone = fixture.nativeElement.querySelector('.preview-phone') as HTMLElement;
    expect(phone.getAttribute('data-wgt-theme')).toBe('dark');
  });

  it('defaults data-wgt-theme to light when not provided', async () => {
    const fixture = await createFixture({});
    const phone = fixture.nativeElement.querySelector('.preview-phone') as HTMLElement;
    expect(phone.getAttribute('data-wgt-theme')).toBe('light');
  });

  it('renders the header with display name from formState', async () => {
    const fixture = await createFixture({ displayName: 'My Support', name: 'Support Widget' });
    const headerName = fixture.nativeElement.querySelector('.wgt-header-name');
    expect(headerName.textContent).toContain('My Support');
  });

  it('falls back to name when displayName is empty', async () => {
    const fixture = await createFixture({ name: 'Support Widget' });
    const headerName = fixture.nativeElement.querySelector('.wgt-header-name');
    expect(headerName.textContent).toContain('Support Widget');
  });

  it('renders welcome message from formState', async () => {
    const fixture = await createFixture({ welcomeMessage: 'Welcome to support!' });
    const bubbles = fixture.nativeElement.querySelectorAll('.wgt-bubble');
    const lastBubble = bubbles[bubbles.length - 1];
    expect(lastBubble.textContent).toContain('Welcome to support!');
  });

  it('renders default welcome message when not provided', async () => {
    const fixture = await createFixture({});
    const bubbles = fixture.nativeElement.querySelectorAll('.wgt-bubble');
    const lastBubble = bubbles[bubbles.length - 1];
    expect(lastBubble.textContent).toContain('Hello! How can we help you today?');
  });

  it('renders the launcher button when enabled is not false', async () => {
    const fixture = await createFixture({ enabled: true });
    expect(fixture.nativeElement.querySelector('.wgt-launcher')).toBeTruthy();
  });

  it('hides the launcher button when enabled is false', async () => {
    const fixture = await createFixture({ enabled: false });
    expect(fixture.nativeElement.querySelector('.wgt-launcher')).toBeFalsy();
  });

  it('renders the launcher by default when enabled is not set', async () => {
    const fixture = await createFixture({});
    expect(fixture.nativeElement.querySelector('.wgt-launcher')).toBeTruthy();
  });

  it('applies primary color to the launcher background', async () => {
    const fixture = await createFixture({ primaryColor: '#FF0000', enabled: true });
    const launcher = fixture.nativeElement.querySelector('.wgt-launcher') as HTMLElement;
    expect(launcher.style.background).toBe('rgb(255, 0, 0)');
  });

  it('renders the composer input and send button', async () => {
    const fixture = await createFixture({});
    expect(fixture.nativeElement.querySelector('.wgt-composer')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('.wgt-input')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('.wgt-send-btn')).toBeTruthy();
  });

  it('renders the close button in the header', async () => {
    const fixture = await createFixture({});
    expect(fixture.nativeElement.querySelector('.wgt-close')).toBeTruthy();
  });
});
