import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { AuthService } from '../../../core/auth/auth.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { CurrentUserService } from '../../../core/tenant/current-user.service';
import { TeamApiService } from '../../tenant/team/team-api.service';
import { AcceptInvitationComponent } from './accept-invitation.component';

describe('AcceptInvitationComponent', () => {
  const mockApi = {
    previewInvitation: vi.fn(),
    acceptInvitation: vi.fn(),
  };
  const mockCurrentUser = {
    currentUser: vi.fn().mockReturnValue(null),
    load: vi.fn().mockResolvedValue(undefined),
  };
  const mockAuth = {
    logout: vi.fn().mockResolvedValue(undefined),
  };
  const mockPermissions = { has: vi.fn().mockReturnValue(true) };

  function setup() {
    TestBed.configureTestingModule({
      imports: [AcceptInvitationComponent],
      providers: [
        provideTaiga(),
        provideRouter([
          { path: 'invite/:token', component: AcceptInvitationComponent },
          { path: 'tenant/team', component: AcceptInvitationComponent },
        ]),
        provideZonelessChangeDetection(),
        { provide: TeamApiService, useValue: mockApi },
        { provide: CurrentUserService, useValue: mockCurrentUser },
        { provide: AuthService, useValue: mockAuth },
        { provide: PermissionsService, useValue: mockPermissions },
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: convertToParamMap({ token: 'test-token' }) } },
        },
      ],
    });
    return TestBed.compileComponents();
  }

  beforeEach(() => {
    mockApi.previewInvitation.mockReset();
    mockApi.acceptInvitation.mockReset();
    mockCurrentUser.currentUser.mockReturnValue(null);
    mockCurrentUser.load.mockReset();
    mockCurrentUser.load.mockResolvedValue(undefined);
    mockAuth.logout.mockReset();
    mockAuth.logout.mockResolvedValue(undefined);
    mockPermissions.has.mockReset();
    mockPermissions.has.mockReturnValue(true);
  });

  it('creates the component', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    expect(fixture.componentInstance).toBeTruthy();
  });

  it('wraps the invitation flow in the auth card shell', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-auth-card')).toBeTruthy();
    });
  });

  it('uses shared controls with programmatically associated form labels and errors', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('form');
    });

    const fields = fixture.nativeElement.querySelectorAll('app-form-field');
    expect(fields).toHaveLength(2);
    expect(fixture.nativeElement.querySelectorAll('app-button').length).toBeGreaterThan(0);

    fixture.componentInstance.form.markAllAsTouched();
    fixture.detectChanges();

    for (const name of ['displayName', 'password']) {
      const input = fixture.nativeElement.querySelector(`[formcontrolname="${name}"]`);
      const label = fixture.nativeElement.querySelector(`label[for="${input.id}"]`);
      expect(label).toBeTruthy();
      expect(input.getAttribute('aria-invalid')).toBe('true');
      const errorId = input.getAttribute('aria-describedby');
      expect(errorId).toBeTruthy();
      expect(fixture.nativeElement.querySelector(`#${errorId}`)).toBeTruthy();
    }
    expect(fixture.nativeElement.querySelectorAll('app-inline-alert [role="alert"]')).toHaveLength(
      2,
    );
  });

  it('shows form step when account does not exist', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('form');
    });
  });

  it('shows preview step when account exists', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('preview');
    });
  });

  it('waits for explicit acceptance when the preview shows an existing account', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-2',
          email: 'user@acme.com',
          displayName: 'User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    expect(mockApi.acceptInvitation).not.toHaveBeenCalled();
    expect(fixture.componentInstance['status']()).toBe('preview');
  });

  it('shows mismatch guidance when signed in with a different email', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'other@example.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('preview');
      expect(fixture.nativeElement.textContent).toContain(
        'This invitation was issued to user@acme.com.',
      );
      expect(fixture.nativeElement.textContent).toContain(
        'Sign out and use that email to accept it.',
      );
      expect(fixture.nativeElement.textContent).toContain('Sign out');
    });
  });

  it('offers a sign-out recovery action when the signed-in email does not match', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'other@example.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('preview');
    });

    fixture.nativeElement.querySelector('button.primary')?.click();
    expect(mockAuth.logout).toHaveBeenCalledWith({ returnUrl: '/invite/test-token' });
  });

  it('sends existing-account users to login with the invitation return URL preserved', async () => {
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('preview');
    });

    fixture.nativeElement.querySelector('button.primary')?.click();

    expect(navigateSpy).toHaveBeenCalledWith(['/', 'auth', 'login'], {
      queryParams: { returnUrl: '/invite/test-token' },
    });
  });

  it('shows mismatch guidance on the registration form when the signed-in email differs', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'other@example.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('form');
      expect(fixture.nativeElement.textContent).toContain(
        'This invitation was issued to user@acme.com.',
      );
      expect(fixture.nativeElement.querySelector('form')).toBeNull();
    });
  });

  it('selects the newly-joined tenant (not any previously-active one) after acceptance', async () => {
    // The user was not a member of "acme" before accepting — this is the tenant the
    // invitation just added them to, and it must become the active tenant context, not
    // whatever was active beforehand.
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-2',
          email: 'user@acme.com',
          displayName: 'User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            {
              tenantId: 'tenant-acme',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    fixture.nativeElement.querySelector('button.primary')?.click();
    await vi.waitFor(() => {
      expect(mockCurrentUser.load).toHaveBeenCalledWith('tenant-acme');
    });
  });

  it('selects the newly-joined tenant even when the user already belongs to a different tenant', async () => {
    // Signed-in user is already a member of tenant-other (perhaps currently active); the
    // invite is for a different tenant (tenant-new). The post-accept membership diff must
    // pick tenant-new, not fall back to whatever was active before.
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [
        {
          tenantId: 'tenant-other',
          tenantName: 'Other Co',
          tenantSlug: 'other',
          role: 'admin',
          permissions: [],
        },
      ],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'New Co',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-1',
          email: 'user@acme.com',
          displayName: 'User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            {
              tenantId: 'tenant-other',
              tenantName: 'Other Co',
              tenantSlug: 'other',
              role: 'admin',
              permissions: [],
            },
            {
              tenantId: 'tenant-new',
              tenantName: 'New Co',
              tenantSlug: 'new',
              role: 'agent',
              permissions: [],
            },
          ],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    fixture.nativeElement.querySelector('button.primary')?.click();
    await vi.waitFor(() => {
      expect(mockCurrentUser.load).toHaveBeenCalledWith('tenant-new');
    });
  });

  it('selects the sole membership after an anonymous signup accepts the invitation', async () => {
    // No signed-in user yet (accountExists: false renders the registration form).
    mockCurrentUser.currentUser.mockReturnValue(null);
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'new@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-new',
          email: 'new@acme.com',
          displayName: 'New Person',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            {
              tenantId: 'tenant-acme',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('form');
    });

    fixture.componentInstance.form.controls.displayName.setValue('New Person');
    fixture.componentInstance.form.controls.password.setValue('password123');
    fixture.detectChanges();
    fixture.nativeElement.querySelector('button.primary')?.click();

    await vi.waitFor(() => {
      expect(mockCurrentUser.load).toHaveBeenCalledWith('tenant-acme');
    });
  });

  it('maps validation errors to the display name and password fields', async () => {
    mockCurrentUser.currentUser.mockReturnValue(null);
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'new@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: false,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      throwError(() => ({
        status: 422,
        message: 'Validation failed',
        details: [
          {
            field: 'display_name',
            code: 'validation_failed',
            message: 'Display name is required.',
          },
          { field: 'password', code: 'validation_failed', message: 'Password is too short.' },
        ],
      })),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('form');
    });

    fixture.componentInstance.form.controls.displayName.setValue('New Person');
    fixture.componentInstance.form.controls.password.setValue('password123');
    fixture.detectChanges();
    fixture.nativeElement.querySelector('button.primary')?.click();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Display name is required.');
      expect(fixture.nativeElement.textContent).toContain('Password is too short.');
    });
  });

  it('navigates to the team page when members.view is granted', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-2',
          email: 'user@acme.com',
          displayName: 'User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            {
              tenantId: 'tenant-acme',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);

    fixture.nativeElement.querySelector('button.primary')?.click();
    await vi.waitFor(() => {
      expect(navigateSpy).toHaveBeenCalledWith('/tenant/team');
    });
  });

  it('falls back to the first permitted tenant page when team access is unavailable', async () => {
    mockPermissions.has.mockImplementation((perm: string) => perm === 'overview.view');
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      of({
        data: {
          id: 'u-2',
          email: 'user@acme.com',
          displayName: 'User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            {
              tenantId: 'tenant-acme',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
        },
      }),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);

    fixture.nativeElement.querySelector('button.primary')?.click();
    await vi.waitFor(() => {
      expect(navigateSpy).toHaveBeenCalledWith('/tenant/overview');
    });
  });

  it('shows error state when preview fails with NOT_FOUND', async () => {
    mockApi.previewInvitation.mockReturnValue(
      throwError(() => ({ code: 'NOT_FOUND', message: 'Not found' })),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('error');
      expect(fixture.nativeElement.textContent).toContain('invalid or has expired');
    });
  });

  it('shows error state when preview fails with INVITATION_EXPIRED', async () => {
    mockApi.previewInvitation.mockReturnValue(
      throwError(() => ({ code: 'INVITATION_EXPIRED', message: 'Expired' })),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('error');
      expect(fixture.nativeElement.textContent).toContain('has expired');
      expect(fixture.nativeElement.textContent).toContain(
        'Ask a workspace admin to issue a fresh invitation',
      );
      expect(fixture.nativeElement.textContent).not.toContain('Sign in');
    });
  });

  it('shows administrator re-enablement guidance without sign-out as the primary disabled-membership recovery', async () => {
    mockCurrentUser.currentUser.mockReturnValue({
      id: 'u-1',
      email: 'user@acme.com',
      displayName: 'Signed In',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    });
    mockApi.previewInvitation.mockReturnValue(
      of({
        data: {
          tenantName: 'Acme Corp',
          email: 'user@acme.com',
          role: 'agent',
          expiresAt: '2026-02-01T00:00:00Z',
          accountExists: true,
        },
      }),
    );
    mockApi.acceptInvitation.mockReturnValue(
      throwError(() => ({ status: 409, message: 'Your account is disabled in this tenant' })),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('preview');
    });

    fixture.nativeElement.querySelector('button.primary')?.click();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('error');
      expect(fixture.nativeElement.textContent).toContain(
        'Ask a workspace Owner or Admin to re-enable your existing membership',
      );
      expect(fixture.nativeElement.textContent).not.toContain('Sign out');
      expect(fixture.nativeElement.querySelector('form')).toBeNull();
    });
  });

  it('shows error state when preview fails with INVITATION_ACCEPTED', async () => {
    mockApi.previewInvitation.mockReturnValue(
      throwError(() => ({ code: 'INVITATION_ACCEPTED', message: 'Already accepted' })),
    );
    await setup();
    const fixture = TestBed.createComponent(AcceptInvitationComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance['status']()).toBe('error');
      expect(fixture.nativeElement.textContent).toContain('already been accepted');
    });
  });
});
