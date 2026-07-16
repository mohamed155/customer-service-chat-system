import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { AiHandlingBannerComponent } from './ai-handling-banner.component';

describe('AiHandlingBannerComponent', () => {
  async function createComponent(config?: {
    visible?: boolean;
    platformAiUnavailable?: boolean;
    platformAiUnavailableReason?: string | null;
  }) {
    await TestBed.configureTestingModule({
      imports: [AiHandlingBannerComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();

    const fixture = TestBed.createComponent(AiHandlingBannerComponent);
    const component = fixture.componentInstance;

    if (config) {
      if (config.visible !== undefined) fixture.componentRef.setInput('visible', config.visible);
      if (config.platformAiUnavailable !== undefined)
        fixture.componentRef.setInput('platformAiUnavailable', config.platformAiUnavailable);
      if (config.platformAiUnavailableReason !== undefined)
        fixture.componentRef.setInput(
          'platformAiUnavailableReason',
          config.platformAiUnavailableReason,
        );
    }

    fixture.detectChanges();
    return { fixture, component };
  }

  it('renders banner when visible is true', async () => {
    const { fixture } = await createComponent({ visible: true });
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('awaiting an AI-handling decision');
    expect(el.querySelector('button')).toBeTruthy();
  });

  it('does not render banner when visible is false', async () => {
    const { fixture } = await createComponent({ visible: false });
    expect(fixture.nativeElement.querySelector('.ai-handling-banner')).toBeFalsy();
  });

  it('disables Platform AI button when platformAiUnavailable is true', async () => {
    const { fixture } = await createComponent({ visible: true, platformAiUnavailable: true });
    const buttons = fixture.nativeElement.querySelectorAll('button');
    expect(buttons[0].disabled).toBe(true);
  });

  it('emits choosePlatformAi when clicking Use Platform AI button', async () => {
    const { fixture, component } = await createComponent({ visible: true });
    const spy = vi.fn();
    component.choosePlatformAi.subscribe(spy);

    const buttons = fixture.nativeElement.querySelectorAll('button');
    buttons[0].click();

    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('emits chooseHuman when clicking Assign to a Human button', async () => {
    const { fixture, component } = await createComponent({ visible: true });
    const spy = vi.fn();
    component.chooseHuman.subscribe(spy);

    const buttons = fixture.nativeElement.querySelectorAll('button');
    buttons[1].click();

    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('shows reason text when platformAiUnavailableReason is set', async () => {
    const { fixture } = await createComponent({
      visible: true,
      platformAiUnavailableReason: 'No AI provider configured',
    });
    expect(fixture.nativeElement.textContent).toContain('No AI provider configured');
  });

  it('hides reason text when platformAiUnavailableReason is null', async () => {
    const { fixture } = await createComponent({
      visible: true,
      platformAiUnavailableReason: null,
    });
    expect(fixture.nativeElement.querySelector('.reason')).toBeFalsy();
  });
});
