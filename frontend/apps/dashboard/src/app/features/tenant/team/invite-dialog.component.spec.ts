import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { APP_CONFIG } from '../../../core/config/app-config';
import { InviteDialogComponent } from './invite-dialog.component';

describe('InviteDialogComponent', () => {
  async function setup(): Promise<{
    component: InviteDialogComponent;
    fixture: import('@angular/core/testing').ComponentFixture<InviteDialogComponent>;
  }> {
    TestBed.configureTestingModule({
      imports: [InviteDialogComponent],
      providers: [
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: { publicDashboardUrl: 'https://dashboard.example.com' } },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(InviteDialogComponent);
    fixture.detectChanges();
    return { component: fixture.componentInstance, fixture };
  }

  it('creates the component', async () => {
    const { component } = await setup();
    expect(component).toBeTruthy();
  });

  it('starts in form step', async () => {
    const { component } = await setup();
    expect(component['step']()).toBe('form');
  });

  it('shows result step when result input is set', async () => {
    const { component, fixture } = await setup();
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'sent',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: 'https://example.com/invite/abc123',
      emailSent: true,
      emailDeliveryStatus: 'sent',
    });
    fixture.detectChanges();
    expect(component['step']()).toBe('result');
  });

  it('shows a manual-link fallback when email delivery is unavailable', async () => {
    const { fixture } = await setup();
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'unconfigured',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'unconfigured',
    });
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('We did not send an email');
  });

  it('does not claim queued email was sent', async () => {
    const { fixture } = await setup();
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'queued',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'queued',
    });
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('email is queued');
    expect(fixture.nativeElement.textContent).not.toContain('email has been sent');
  });

  it('surfaces exhausted delivery polling errors in the result step', async () => {
    const { fixture } = await setup();
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'queued',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'queued',
    });
    fixture.componentRef.setInput('deliveryPollingError', 'Unable to refresh delivery status.');
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Unable to refresh delivery status.');
  });

  it('copies the resolved public dashboard invitation URL', async () => {
    const { fixture } = await setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'queued',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: 'invite/abc123',
      emailSent: true,
      emailDeliveryStatus: 'queued',
    });
    fixture.detectChanges();

    (
      fixture.nativeElement.querySelector(
        '[aria-label="Copy invitation link"]',
      ) as HTMLButtonElement
    ).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(writeText).toHaveBeenCalledWith('https://dashboard.example.com/invite/abc123');
    const feedback = fixture.nativeElement.querySelector('app-inline-alert p') as HTMLElement;
    expect(feedback.textContent).toContain('Invitation link copied.');
    expect(feedback.getAttribute('role')).toBe('status');
    expect(feedback.getAttribute('aria-live')).toBe('polite');
    const fallback = fixture.nativeElement.querySelector(
      '[aria-label="Invitation link for manual copying"]',
    ) as HTMLInputElement;
    expect(fallback.value).toBe('https://dashboard.example.com/invite/abc123');
    expect(fallback.readOnly).toBe(true);
    expect(fallback.getAttribute('aria-describedby')).toBe('invite-link-instructions');
    expect(fallback.tabIndex).toBe(0);
  });

  it('announces clipboard failures and keeps the manual-copy link visible', async () => {
    const { fixture } = await setup();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: vi.fn().mockRejectedValue(new Error('Clipboard denied')) },
      configurable: true,
    });
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'failed',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'failed',
    });
    fixture.detectChanges();

    (
      fixture.nativeElement.querySelector(
        '[aria-label="Copy invitation link"]',
      ) as HTMLButtonElement
    ).click();
    await fixture.whenStable();
    fixture.detectChanges();

    const feedback = fixture.nativeElement.querySelector('app-inline-alert p') as HTMLElement;
    expect(feedback.textContent).toContain('Could not copy the invitation link.');
    expect(feedback.textContent).toContain('Select and copy it manually.');
    expect(feedback.getAttribute('role')).toBe('alert');
    expect(feedback.getAttribute('aria-live')).toBe('assertive');
    expect(fixture.nativeElement.querySelector('input[readonly]').value).toBe(
      'https://dashboard.example.com/invite/abc123',
    );
    expect(fixture.nativeElement.querySelector('#invite-link-instructions').textContent).toContain(
      'Select the link to copy it manually',
    );
  });

  it('resets failed feedback and reports success when copying is retried', async () => {
    const { fixture } = await setup();
    const writeText = vi
      .fn()
      .mockRejectedValueOnce(new Error('Clipboard denied'))
      .mockResolvedValueOnce(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'unconfigured',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'unconfigured',
    });
    fixture.detectChanges();

    const copyButton = fixture.nativeElement.querySelector(
      '[aria-label="Copy invitation link"]',
    ) as HTMLButtonElement;
    copyButton.click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Could not copy the invitation link.');

    copyButton.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(writeText).toHaveBeenCalledTimes(2);
    expect(fixture.nativeElement.textContent).toContain('Invitation link copied.');
    expect(fixture.nativeElement.textContent).not.toContain('Could not copy the invitation link.');
  });

  it('reports failure when the Clipboard API is unavailable', async () => {
    const { fixture } = await setup();
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'unconfigured',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'unconfigured',
    });
    fixture.detectChanges();

    (
      fixture.nativeElement.querySelector(
        '[aria-label="Copy invitation link"]',
      ) as HTMLButtonElement
    ).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Could not copy the invitation link.');
    expect(fixture.nativeElement.querySelector('input[readonly]').value).toContain(
      '/invite/abc123',
    );
  });

  it('ignores a stale failure when a newer overlapping copy succeeds', async () => {
    const { fixture } = await setup();
    let rejectFirst!: (reason: Error) => void;
    let resolveSecond!: () => void;
    const first = new Promise<void>((_resolve, reject) => (rejectFirst = reject));
    const second = new Promise<void>((resolve) => (resolveSecond = resolve));
    const writeText = vi.fn().mockReturnValueOnce(first).mockReturnValueOnce(second);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    fixture.componentRef.setInput('result', {
      invitation: {
        id: 'i-1',
        email: 'test@test.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Admin',
        emailDeliveryStatus: 'queued',
        createdAt: '2026-01-01T00:00:00Z',
        expiresAt: '2026-02-01T00:00:00Z',
      },
      acceptUrl: '/invite/abc123',
      emailSent: false,
      emailDeliveryStatus: 'queued',
    });
    fixture.detectChanges();
    const copyButton = fixture.nativeElement.querySelector(
      '[aria-label="Copy invitation link"]',
    ) as HTMLButtonElement;

    copyButton.click();
    copyButton.click();
    resolveSecond();
    await second;
    rejectFirst(new Error('stale failure'));
    await first.catch(() => undefined);
    await fixture.whenStable();
    fixture.detectChanges();

    expect(writeText).toHaveBeenCalledTimes(2);
    expect(fixture.nativeElement.textContent).toContain('Invitation link copied.');
    expect(fixture.nativeElement.textContent).not.toContain('Could not copy the invitation link.');
  });

  it('renders dialog semantics and closes on Escape', async () => {
    const { fixture } = await setup();
    const closeSpy = vi.fn();
    fixture.componentInstance.closeDialog.subscribe(closeSpy);

    const dialog = fixture.nativeElement.querySelector('.dialog') as HTMLElement;
    expect(dialog.getAttribute('role')).toBe('dialog');
    expect(dialog.getAttribute('aria-modal')).toBe('true');

    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(closeSpy).toHaveBeenCalled();
  });

  it('renders through the shared dialog shell', async () => {
    const { fixture } = await setup();

    expect(fixture.nativeElement.querySelector('app-dialog-shell')).toBeTruthy();
  });

  it('uses shared form, button, and alert primitives inside the dialog', async () => {
    const { fixture } = await setup();

    expect(fixture.nativeElement.querySelector('app-form-field')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('app-button')).toBeTruthy();

    fixture.componentRef.setInput('error', 'Duplicate invitation');
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-inline-alert')).toBeTruthy();
  });

  it('emits invite on submit when email is provided', async () => {
    const { component } = await setup();
    const inviteSpy = vi.fn();
    component.invite.subscribe(inviteSpy);

    component['form'].controls.email.setValue('user@test.com');
    component['submit']();

    expect(inviteSpy).toHaveBeenCalledWith({
      email: 'user@test.com',
      role: 'agent',
    });
  });

  it('does not emit invite when email is empty', async () => {
    const { component } = await setup();
    const inviteSpy = vi.fn();
    component.invite.subscribe(inviteSpy);

    component['form'].controls.email.setValue('');
    component['submit']();

    expect(inviteSpy).not.toHaveBeenCalled();
  });

  it('emits close event', async () => {
    const { component } = await setup();
    const closeSpy = vi.fn();
    component.closeDialog.subscribe(closeSpy);

    component.closeDialog.emit();

    expect(closeSpy).toHaveBeenCalled();
  });
});
