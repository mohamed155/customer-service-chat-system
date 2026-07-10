import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { ApiService } from '../../core/api/api.service';
import { AuthService } from '../../core/auth/auth.service';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { UserMenuComponent } from './user-menu.component';

import { MeResponse, MembershipRole, PlatformRole } from '../../core/api/tenant-api.models';

const baseUser: MeResponse = {
  id: 'u-1',
  email: 'user@test.com',
  displayName: 'Test User',
  platformRole: null,
  platformPermissions: [],
  staffTenantPermissions: null,
  memberships: [],
};

describe('UserMenuComponent', () => {
  async function setup(opts?: {
    platformRole?: PlatformRole | null;
    tenantRole?: MembershipRole | null;
  }) {
    const auth: AuthService = { logout: vi.fn() } as unknown as AuthService;
    const platformRole = opts?.platformRole ?? null;
    const tenantRole = opts?.tenantRole ?? null;

    TestBed.configureTestingModule({
      imports: [UserMenuComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { tenantContext: { activeTenant: null, status: 'idle' as const } },
        }),
        CurrentUserService,
        { provide: ApiService, useValue: { get: vi.fn(), post: vi.fn() } },
        { provide: AuthService, useValue: auth },
      ],
    });

    const currentUser = TestBed.inject(CurrentUserService);
    currentUser['user'].set({
      ...baseUser,
      platformRole,
      memberships: tenantRole
        ? [
            {
              tenantId: 't-1',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: tenantRole,
              permissions: [],
            },
          ]
        : [],
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(UserMenuComponent);
    fixture.detectChanges();
    return { fixture, auth, currentUser, store: TestBed.inject(MockStore) };
  }

  it('creates the component', async () => {
    const { fixture } = await setup();
    expect(fixture.componentInstance).toBeTruthy();
  });

  it('renders avatar with user initials', async () => {
    const { fixture } = await setup();
    const avatar = (fixture.nativeElement as HTMLElement).querySelector('app-avatar')!;
    expect(avatar.textContent!.trim()).toBe('TU');
  });

  it('shows display name and email when open', async () => {
    const { fixture } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();

    const text = (fixture.nativeElement as HTMLElement).textContent!;
    expect(text).toContain('Test User');
    expect(text).toContain('user@test.com');
  });

  describe('role line', () => {
    it('shows platform role name for platform staff', async () => {
      const { fixture } = await setup({ platformRole: 'super_admin' });
      const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
      trigger.dispatchEvent(new Event('click'));
      fixture.detectChanges();

      expect((fixture.nativeElement as HTMLElement).textContent).toContain('Super Admin');
    });

    it('shows role and tenant name for tenant user', async () => {
      const { fixture, store } = await setup({ tenantRole: 'agent' });
      store.setState({
        tenantContext: {
          activeTenant: { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active' },
          status: 'idle' as const,
        },
      });
      fixture.detectChanges();

      const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
      trigger.dispatchEvent(new Event('click'));
      fixture.detectChanges();

      expect((fixture.nativeElement as HTMLElement).textContent).toContain(
        'Support Agent · Acme Corp',
      );
    });

    it('omits role line when no role is available', async () => {
      const { fixture } = await setup();
      const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
      trigger.dispatchEvent(new Event('click'));
      fixture.detectChanges();

      expect((fixture.nativeElement as HTMLElement).querySelector('.role-line')).toBeNull();
    });
  });

  it('opens and closes dropdown on trigger click', async () => {
    const { fixture } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;

    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });

  it('has correct aria attributes on trigger', async () => {
    const { fixture } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;

    expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
  });

  it('calls auth.logout on sign out', async () => {
    const { fixture, auth } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();

    const signOutBtn = (fixture.nativeElement as HTMLElement).querySelector('.sign-out')!;
    signOutBtn.dispatchEvent(new Event('click'));
    fixture.detectChanges();

    expect(auth.logout).toHaveBeenCalledOnce();
  });

  it('closes dropdown on outside click', async () => {
    const { fixture } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;

    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });

  it('closes dropdown on escape key', async () => {
    const { fixture } = await setup();
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;

    trigger.dispatchEvent(new Event('click'));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    fixture.nativeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'escape' }));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });
});
