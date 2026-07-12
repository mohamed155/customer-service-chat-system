import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of } from 'rxjs';
import { ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { TenantSummary } from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore } from './tenants.store';
import { TenantListComponent } from './tenant-list.component';

const emptyPage = (): ApiResponse<PaginatedResponse<TenantSummary>> => ({
  data: { items: [], nextCursor: null, hasMore: false },
});

describe('TenantsStore (providedIn: "root" provider resolution)', () => {
  let service: { list: ReturnType<typeof vi.fn> };

  beforeEach(async () => {
    service = {
      list: vi.fn().mockReturnValue(of(emptyPage())),
    };

    await TestBed.configureTestingModule({
      imports: [TenantListComponent],
      providers: [
        provideRouter([]),
        provideTaiga(),
        { provide: PlatformTenantsService, useValue: service },
        { provide: PermissionsService, useValue: { has: vi.fn().mockReturnValue(false) } },
      ],
    }).compileComponents();
  });

  it('resolves TenantsStore via the root injector for a routed component', () => {
    TestBed.createComponent(TenantListComponent);

    const store = TestBed.inject(TenantsStore);
    expect(store).toBeTruthy();
    expect(typeof store.load).toBe('function');
    expect(typeof store.setQueryInput).toBe('function');
    expect(typeof store.setStatusFilter).toBe('function');
    expect(typeof store.loadMore).toBe('function');
  });

  it('uses the providedIn: "root" store inside the routed component (no explicit provider)', () => {
    const fixture = TestBed.createComponent(TenantListComponent);
    const componentStore = (fixture.componentInstance as unknown as { store: unknown }).store;
    const injectedStore = TestBed.inject(TenantsStore);
    expect(componentStore).toBe(injectedStore);
  });

  it('invokes PlatformTenantsService.list when store.load() is called on the resolved store', () => {
    TestBed.createComponent(TenantListComponent);

    const store = TestBed.inject(TenantsStore);
    const callsBefore = service.list.mock.calls.length;

    store.load();

    expect(service.list.mock.calls.length).toBe(callsBefore + 1);
    expect(service.list).toHaveBeenLastCalledWith({
      q: undefined,
      status: undefined,
      cursor: undefined,
      limit: 25,
    });
  });
});
