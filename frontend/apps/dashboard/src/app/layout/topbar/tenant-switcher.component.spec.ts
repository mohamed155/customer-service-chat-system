import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { ApiService } from '../../core/api/api.service';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { TenantSummary } from '../../core/api/tenant-api.models';
import { TenantSwitcherComponent } from './tenant-switcher.component';

const fakeTenants: TenantSummary[] = [
  { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'trial' },
  { id: 't-2', name: 'Globex Inc', slug: 'globex', status: 'active', plan: 'starter' },
  { id: 't-3', name: 'Initech', slug: 'initech', status: 'suspended', plan: 'professional' },
];

describe('TenantSwitcherComponent', () => {
  async function setup(isPlatform: boolean) {
    const api = { list: vi.fn(), post: vi.fn() };
    api.list.mockReturnValue(
      of({ data: { items: fakeTenants, nextCursor: null, hasMore: false } }),
    );

    TestBed.configureTestingModule({
      imports: [TenantSwitcherComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { tenantContext: { activeTenant: null, status: 'idle' as const } },
        }),
        CurrentUserService,
        { provide: ApiService, useValue: api },
      ],
    });

    const currentUser = TestBed.inject(CurrentUserService);
    currentUser['user'].set({
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: isPlatform ? 'super_admin' : null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: isPlatform
        ? []
        : [
            {
              tenantId: 't-1',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TenantSwitcherComponent);
    fixture.detectChanges();
    return { fixture, api, store: TestBed.inject(MockStore) };
  }

  it('loads tenants on init', async () => {
    const { fixture } = await setup(true);
    const element = fixture.nativeElement as HTMLElement;
    expect(element.textContent).toContain('Select tenant...');
  });

  it('opens dropdown when trigger is clicked', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    const dropdown = (fixture.nativeElement as HTMLElement).querySelector('.dropdown');
    expect(dropdown).toBeTruthy();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Acme Corp');
  });

  it('displays active tenant name when one is selected', async () => {
    const { fixture, store } = await setup(true);
    store.setState({
      tenantContext: {
        activeTenant: {
          id: 't-1',
          name: 'Acme Corp',
          slug: 'acme',
          status: 'active',
          plan: 'trial',
        },
        status: 'idle' as const,
      },
    });
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Acme Corp');
  });

  it('calls tenantContext.select on tenant click', async () => {
    const { fixture, api, store } = await setup(true);
    const dispatch = vi.spyOn(store, 'dispatch');

    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();

    api.post.mockReturnValue(of({ data: fakeTenants[0] }));

    const firstOption = (fixture.nativeElement as HTMLElement).querySelector(
      '.option',
    ) as HTMLElement;
    firstOption.click();

    expect(api.post).toHaveBeenCalledWith('/platform/tenants/t-1/switch', undefined);
    expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ tenantId: 't-1' }));
  });

  it('has aria-haspopup="listbox" on trigger', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger')!;
    expect(trigger.getAttribute('aria-haspopup')).toBe('listbox');
  });

  it('has role="option" and aria-selected on options', async () => {
    const { fixture, store } = await setup(true);
    store.setState({
      tenantContext: {
        activeTenant: {
          id: 't-1',
          name: 'Acme Corp',
          slug: 'acme',
          status: 'active',
          plan: 'trial',
        },
        status: 'idle' as const,
      },
    });
    fixture.detectChanges();

    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();

    const options = (fixture.nativeElement as HTMLElement).querySelectorAll('.option');
    expect(options.length).toBeGreaterThan(0);
    expect(options[0].getAttribute('role')).toBe('option');
    expect(options[0].getAttribute('aria-selected')).toBe('true');
  });

  it('closes dropdown on escape key', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    fixture.nativeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'escape' }));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });
});
