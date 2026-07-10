import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { Permission } from './permissions';
import { permissionGuard } from './permission.guard';
import { PermissionsService } from './permissions.service';

describe('permissionGuard', () => {
  let permissions: { has: ReturnType<typeof vi.fn> };
  let router: { navigateByUrl: ReturnType<typeof vi.fn>; createUrlTree: ReturnType<typeof vi.fn> };

  const configure = (hasResults: Record<string, boolean>) => {
    permissions = { has: vi.fn((p: Permission) => hasResults[p] ?? false) };
    router = { navigateByUrl: vi.fn(), createUrlTree: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: PermissionsService, useValue: permissions },
        { provide: Router, useValue: router },
      ],
    });
  };

  it('allows when the required permission is held', () => {
    configure({ 'overview.view': true });
    const result = TestBed.runInInjectionContext(() =>
      permissionGuard(
        { data: { requiredPermission: 'overview.view' as Permission }, path: '' } as never,
        [],
        {} as never,
      ),
    );
    expect(result).toBe(true);
  });

  it('denies (fail-closed) when requiredPermission data key is missing', () => {
    configure({});
    const result = TestBed.runInInjectionContext(() =>
      permissionGuard({ path: '' } as never, [], {} as never),
    );
    expect(result).toBe(false);
  });

  it('redirects to first permitted page when permission is not held', () => {
    configure({ 'ai_agent.view': true });
    TestBed.runInInjectionContext(() =>
      permissionGuard(
        { data: { requiredPermission: 'overview.view' as Permission }, path: '' } as never,
        [],
        {} as never,
      ),
    );
    expect(router.createUrlTree).toHaveBeenCalledWith(['/tenant/ai-agent']);
  });

  it('falls back to /tenant/select when no page permission is held', () => {
    configure({});
    TestBed.runInInjectionContext(() =>
      permissionGuard(
        { data: { requiredPermission: 'overview.view' as Permission }, path: '' } as never,
        [],
        {} as never,
      ),
    );
    expect(router.createUrlTree).toHaveBeenCalledWith(['/', 'tenant', 'select']);
  });
});
