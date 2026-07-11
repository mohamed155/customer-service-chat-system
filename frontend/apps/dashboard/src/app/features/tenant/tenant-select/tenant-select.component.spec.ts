import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { TenantSelectComponent } from './tenant-select.component';
import { CurrentUserService } from '../../../core/tenant/current-user.service';

describe('TenantSelectComponent', () => {
  let currentUser: { currentUser: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    currentUser = { currentUser: vi.fn() };
  });

  const configure = () =>
    TestBed.configureTestingModule({
      imports: [TenantSelectComponent],
      providers: [
        provideTaiga(),
        provideRouter([]),
        provideZonelessChangeDetection(),
        { provide: CurrentUserService, useValue: currentUser },
      ],
    });

  it('shows tenant selection copy when user has memberships', async () => {
    currentUser.currentUser.mockReturnValue({
      memberships: [{ tenantId: 't-1', tenantName: 'Acme', tenantSlug: 'acme', role: 'admin' }],
    });
    configure();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TenantSelectComponent);
    fixture.detectChanges();
    const el: HTMLElement = fixture.nativeElement;

    expect(el.textContent).toContain('Select a tenant to get started');
    expect(el.textContent).toContain('Back to tenant area');
    expect(el.querySelector('app-page-header')).not.toBeNull();
    expect(el.textContent).toContain('Select a workspace');
  });

  it('shows no-access copy when user has no memberships', async () => {
    currentUser.currentUser.mockReturnValue({ memberships: [] });
    configure();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TenantSelectComponent);
    fixture.detectChanges();
    const el: HTMLElement = fixture.nativeElement;

    expect(el.textContent).toContain('No workspace access');
    expect(el.textContent).not.toContain('Back to tenant area');
  });

  it('shows no-access copy when user is null', async () => {
    currentUser.currentUser.mockReturnValue(null);
    configure();
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TenantSelectComponent);
    fixture.detectChanges();
    const el: HTMLElement = fixture.nativeElement;

    expect(el.textContent).toContain('No workspace access');
    expect(el.textContent).not.toContain('Back to tenant area');
  });
});
