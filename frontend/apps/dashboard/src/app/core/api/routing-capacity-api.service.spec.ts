import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { ApiService } from './api.service';
import { RoutingCapacityApiService } from './routing-capacity-api.service';

describe('RoutingCapacityApiService', () => {
  const api = {
    get: vi.fn(),
    put: vi.fn(),
  };

  beforeEach(() => {
    api.get.mockReset();
    api.put.mockReset();
    TestBed.configureTestingModule({
      providers: [RoutingCapacityApiService, { provide: ApiService, useValue: api }],
    });
  });

  it('reads the tenant routing capacity contract', () => {
    api.get.mockReturnValue(of({ data: { default_capacity: 10, agents: [] } }));
    const service = TestBed.inject(RoutingCapacityApiService);

    service.get().subscribe();

    expect(api.get).toHaveBeenCalledWith('tenant/routing/capacity');
  });

  it('writes the tenant default capacity with the API field name', () => {
    api.put.mockReturnValue(of({ data: { default_capacity: 12, agents: [] } }));
    const service = TestBed.inject(RoutingCapacityApiService);

    service.setDefault(12).subscribe();

    expect(api.put).toHaveBeenCalledWith('tenant/routing/capacity/default', {
      default_capacity: 12,
    });
  });

  it('writes numeric and inherited agent overrides', () => {
    api.put.mockReturnValue(of({ data: { default_capacity: 10, agents: [] } }));
    const service = TestBed.inject(RoutingCapacityApiService);

    service.setAgent('membership-1', 0).subscribe();
    service.setAgent('membership-1', null).subscribe();

    expect(api.put).toHaveBeenNthCalledWith(1, 'tenant/routing/capacity/agents/membership-1', {
      capacity: 0,
    });
    expect(api.put).toHaveBeenNthCalledWith(2, 'tenant/routing/capacity/agents/membership-1', {
      capacity: null,
    });
  });
});
