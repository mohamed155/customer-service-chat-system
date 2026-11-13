import { provideZonelessChangeDetection, signal, WritableSignal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { AuthService, AuthSignupError } from '../../../core/auth/auth.service';
import { SignupComponent } from './signup.component';

describe('SignupComponent', () => {
  let auth: { signup: ReturnType<typeof vi.fn>; pending: WritableSignal<boolean> };
  let fixture: ComponentFixture<SignupComponent>;
  let router: Router;

  beforeEach(async () => {
    auth = { signup: vi.fn(), pending: signal(false) };

    await TestBed.configureTestingModule({
      imports: [SignupComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: AuthService, useValue: auth },
      ],
    }).compileComponents();

    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);
    fixture = TestBed.createComponent(SignupComponent);
    fixture.detectChanges();
  });

  it('renders the branded auth card and labeled fields', () => {
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).toContain('Helix');
    expect(element.textContent).toContain('Create workspace');
    expect(input(fixture, 'signup-workspace-name')).toBeTruthy();
    expect(input(fixture, 'signup-display-name')).toBeTruthy();
    expect(element.querySelector('input[type="email"]')).toBeTruthy();
    expect(element.querySelector('input[type="password"]')).toBeTruthy();
  });

  it('does not submit invalid forms and shows validation errors', async () => {
    form(fixture).dispatchEvent(new Event('submit'));
    fixture.detectChanges();
    await fixture.whenStable();

    expect(auth.signup).not.toHaveBeenCalled();
    expect(fixture.nativeElement.textContent).toContain('Workspace name is required');
  });

  it('rejects a password shorter than 8 characters', async () => {
    setInput(fixture, 'signup-workspace-name', 'Acme Corp');
    setInput(fixture, 'signup-display-name', 'Jane Doe');
    setInput(fixture, '#signup-email', 'jane@example.com');
    setInput(fixture, 'signup-password', 'short');
    form(fixture).dispatchEvent(new Event('submit'));
    fixture.detectChanges();
    await fixture.whenStable();

    expect(auth.signup).not.toHaveBeenCalled();
    expect(fixture.nativeElement.textContent).toContain('Password must be at least 8 characters');
  });

  it('submits a valid signup and navigates to the tenant overview', async () => {
    auth.signup.mockResolvedValue(undefined);

    setInput(fixture, 'signup-workspace-name', 'Acme Corp');
    setInput(fixture, 'signup-display-name', 'Jane Doe');
    setInput(fixture, '#signup-email', 'jane@example.com');
    setInput(fixture, 'signup-password', 'correct horse battery');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();

    expect(auth.signup).toHaveBeenCalledWith({
      workspaceName: 'Acme Corp',
      displayName: 'Jane Doe',
      email: 'jane@example.com',
      password: 'correct horse battery',
    });
    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/overview');
  });

  it('shows a friendly message and does not navigate on a 409 email conflict', async () => {
    auth.signup.mockRejectedValue(
      new AuthSignupError('auth.errors.accountExists', {
        code: 'conflict',
        message: 'raw',
        status: 409,
      }),
    );

    setInput(fixture, 'signup-workspace-name', 'Acme Corp');
    setInput(fixture, 'signup-display-name', 'Jane Doe');
    setInput(fixture, '#signup-email', 'jane@example.com');
    setInput(fixture, 'signup-password', 'correct horse battery');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('already exists');
    expect(router.navigateByUrl).not.toHaveBeenCalled();
  });

  it('maps 422 field errors from the server onto the matching control', async () => {
    auth.signup.mockRejectedValue(
      new AuthSignupError('errors.requestFailed', {
        code: 'validation_failed',
        message: 'Please correct the highlighted fields.',
        status: 422,
        details: [{ field: 'email', code: 'invalid_format', message: 'Email is already taken' }],
      }),
    );

    setInput(fixture, 'signup-workspace-name', 'Acme Corp');
    setInput(fixture, 'signup-display-name', 'Jane Doe');
    setInput(fixture, '#signup-email', 'jane@example.com');
    setInput(fixture, 'signup-password', 'correct horse battery');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();
    fixture.detectChanges();

    // The field shows the text for its error code, never the API's own message.
    expect(fixture.nativeElement.textContent).toContain('This value is not valid.');
    expect(fixture.nativeElement.textContent).not.toContain('Email is already taken');
  });

  it('disables resubmission while pending', () => {
    auth.pending.set(true);
    fixture.detectChanges();

    expect(button(fixture).disabled).toBe(true);
    expect(button(fixture).textContent).toContain('Creating workspace...');
  });
});

const form = (fixture: ComponentFixture<SignupComponent>): HTMLFormElement =>
  fixture.nativeElement.querySelector('form');

const button = (fixture: ComponentFixture<SignupComponent>): HTMLButtonElement =>
  fixture.nativeElement.querySelector('button[type="submit"]');

const input = (fixture: ComponentFixture<SignupComponent>, id: string): HTMLInputElement =>
  fixture.nativeElement.querySelector(`#${id}`);

const setInput = (
  fixture: ComponentFixture<SignupComponent>,
  selector: string,
  value: string,
): void => {
  const normalized = selector.startsWith('#') ? selector : `#${selector}`;
  const element: HTMLInputElement = fixture.nativeElement.querySelector(normalized);
  element.value = value;
  element.dispatchEvent(new Event('input'));
  fixture.detectChanges();
};
