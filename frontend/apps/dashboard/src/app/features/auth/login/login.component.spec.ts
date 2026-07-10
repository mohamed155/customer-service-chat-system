import { provideZonelessChangeDetection, signal, WritableSignal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import {
  AuthLoginError,
  AuthService,
  INVALID_CREDENTIALS_MESSAGE,
} from '../../../core/auth/auth.service';
import { LoginComponent } from './login.component';

describe('LoginComponent', () => {
  let auth: { login: ReturnType<typeof vi.fn>; pending: WritableSignal<boolean> };
  let fixture: ComponentFixture<LoginComponent>;
  let router: Router;

  beforeEach(async () => {
    auth = { login: vi.fn(), pending: signal(false) };

    await TestBed.configureTestingModule({
      imports: [LoginComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: AuthService, useValue: auth },
      ],
    }).compileComponents();

    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);
    fixture = TestBed.createComponent(LoginComponent);
    fixture.detectChanges();
  });

  it('renders the branded auth card and labeled fields', () => {
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).toContain('Helix');
    expect(element.textContent).toContain('Welcome back');
    expect(element.querySelector('input[type="email"]')).toBeTruthy();
    expect(element.querySelector('input[type="password"]')).toBeTruthy();
  });

  it('does not submit invalid forms', async () => {
    form(fixture).dispatchEvent(new Event('submit'));
    fixture.detectChanges();

    await fixture.whenStable();

    expect(auth.login).not.toHaveBeenCalled();
  });

  it('submits credentials and navigates to the safe returnUrl', async () => {
    setReturnUrl(fixture, '/tenant/conversations');
    auth.login.mockResolvedValue(undefined);

    setInput(fixture, 'input[type="email"]', 'agent@example.com');
    setInput(fixture, 'input[type="password"]', 'Passw0rd!');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();

    expect(auth.login).toHaveBeenCalledWith('agent@example.com', 'Passw0rd!');
    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/conversations');
  });

  it('falls back to the tenant overview when returnUrl is external', async () => {
    setReturnUrl(fixture, '//evil.example');
    auth.login.mockResolvedValue(undefined);

    setInput(fixture, 'input[type="email"]', 'agent@example.com');
    setInput(fixture, 'input[type="password"]', 'Passw0rd!');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();

    expect(router.navigateByUrl).toHaveBeenCalledWith('/tenant/overview');
  });

  it('shows the generic invalid credentials message on 401', async () => {
    auth.login.mockRejectedValue(
      new AuthLoginError(INVALID_CREDENTIALS_MESSAGE, {
        code: 'unauthenticated',
        message: 'raw',
        status: 401,
      }),
    );

    setInput(fixture, 'input[type="email"]', 'agent@example.com');
    setInput(fixture, 'input[type="password"]', 'bad');
    form(fixture).dispatchEvent(new Event('submit'));

    await fixture.whenStable();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain(INVALID_CREDENTIALS_MESSAGE);
    expect(router.navigateByUrl).not.toHaveBeenCalled();
  });

  it('disables resubmission while pending', () => {
    auth.pending.set(true);
    fixture.detectChanges();

    expect(button(fixture).disabled).toBe(true);
    expect(button(fixture).textContent).toContain('Signing in...');
  });
});

const form = (fixture: ComponentFixture<LoginComponent>): HTMLFormElement =>
  fixture.nativeElement.querySelector('form');

const button = (fixture: ComponentFixture<LoginComponent>): HTMLButtonElement =>
  fixture.nativeElement.querySelector('button[type="submit"]');

const setInput = (
  fixture: ComponentFixture<LoginComponent>,
  selector: string,
  value: string,
): void => {
  const input: HTMLInputElement = fixture.nativeElement.querySelector(selector);
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
};

const setReturnUrl = (fixture: ComponentFixture<LoginComponent>, returnUrl: string): void => {
  const route = fixture.debugElement.injector.get(ActivatedRoute);
  vi.spyOn(route.snapshot.queryParamMap, 'get').mockImplementation((key: string) =>
    key === 'returnUrl' ? returnUrl : null,
  );
};
