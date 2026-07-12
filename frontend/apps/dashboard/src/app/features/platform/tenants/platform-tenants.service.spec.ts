import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of, throwError } from 'rxjs';
import { ApiError, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  CreateTenantPayload,
  PlatformTenantDetail,
  TenantSummary,
  UpdateTenantPayload,
} from '../../../core/api/tenant-api.models';
import { PlatformTenantsService } from './platform-tenants.service';

describe('PlatformTenantsService', () => {
  let service: PlatformTenantsService;
  let api: {
    list: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = {
      list: vi.fn(),
      get: vi.fn(),
      post: vi.fn(),
      patch: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [PlatformTenantsService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(PlatformTenantsService);
  });

  it('lists tenants via GET /platform/tenants with the provided query', async () => {
    const response: ApiResponse<PaginatedResponse<TenantSummary>> = {
      data: { items: [], nextCursor: null, hasMore: false },
    };
    api.list.mockReturnValue(of(response));

    const result = await firstValueFrom(service.list({ status: 'active', limit: 10 }));

    expect(api.list).toHaveBeenCalledWith('/platform/tenants', { status: 'active', limit: 10 });
    expect(result).toEqual(response);
  });

  it('lists tenants with an empty query by default', async () => {
    api.list.mockReturnValue(of({ data: { items: [], nextCursor: null, hasMore: false } }));

    await firstValueFrom(service.list());

    expect(api.list).toHaveBeenCalledWith('/platform/tenants', {});
  });

  it('fetches a single tenant via GET /platform/tenants/:id', async () => {
    const response: ApiResponse<PlatformTenantDetail> = {
      data: {
        id: 't-1',
        name: 'Acme',
        slug: 'acme',
        status: 'active',
        plan: 'professional',
        contactName: null,
        contactEmail: null,
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
      },
    };
    api.get.mockReturnValue(of(response));

    const result = await firstValueFrom(service.get('t-1'));

    expect(api.get).toHaveBeenCalledWith('/platform/tenants/t-1');
    expect(result).toEqual(response);
  });

  it('creates a tenant via POST /platform/tenants', async () => {
    const payload: CreateTenantPayload = {
      name: 'Acme Corp',
      slug: 'acme',
      plan: 'professional',
      contactEmail: 'ops@acme.test',
    };
    const response: ApiResponse<PlatformTenantDetail> = {
      data: {
        id: 't-2',
        name: 'Acme Corp',
        slug: 'acme',
        status: 'active',
        plan: 'professional',
        contactName: null,
        contactEmail: 'ops@acme.test',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
      },
    };
    api.post.mockReturnValue(of(response));

    const result = await firstValueFrom(service.create(payload));

    expect(api.post).toHaveBeenCalledWith('/platform/tenants', payload);
    expect(result).toEqual(response);
  });

  it('updates a tenant via PATCH /platform/tenants/:id', async () => {
    const payload: UpdateTenantPayload = { name: 'Acme Inc', status: 'suspended' };
    const response: ApiResponse<PlatformTenantDetail> = {
      data: {
        id: 't-1',
        name: 'Acme Inc',
        slug: 'acme',
        status: 'suspended',
        plan: 'starter',
        contactName: null,
        contactEmail: null,
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-02T00:00:00Z',
      },
    };
    api.patch.mockReturnValue(of(response));

    const result = await firstValueFrom(service.update('t-1', payload));

    expect(api.patch).toHaveBeenCalledWith('/platform/tenants/t-1', payload);
    expect(result).toEqual(response);
  });

  it('propagates API errors from the underlying ApiService', async () => {
    const error: ApiError = { code: 'unauthenticated', message: 'No session', status: 401 };
    api.list.mockReturnValue(throwError(() => error));

    await expect(firstValueFrom(service.list())).rejects.toEqual(error);
  });
});
