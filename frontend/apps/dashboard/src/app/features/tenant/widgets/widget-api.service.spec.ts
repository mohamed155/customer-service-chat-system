import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ApiService } from '../../../core/api/api.service';
import { WidgetApiService } from './widget-api.service';
import {
  CreateWidgetInstancePayload,
  UpdateWidgetInstancePayload,
} from '../../../core/api/widget.models';

describe('WidgetApiService', () => {
  let service: WidgetApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    put: ReturnType<typeof vi.fn>;
    delete: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), post: vi.fn(), put: vi.fn(), delete: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        WidgetApiService,
        { provide: ApiService, useValue: api },
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    });
    service = TestBed.inject(WidgetApiService);
  });

  describe('list', () => {
    it('calls api.get with tenant/widgets', async () => {
      const response = { data: [] };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.list());

      expect(api.get).toHaveBeenCalledWith('tenant/widgets');
      expect(result).toEqual(response);
    });
  });

  describe('get', () => {
    it('calls api.get with tenant/widgets/{id}', async () => {
      const response = { data: { id: 'wgt-1', name: 'Test' } };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.get('wgt-1'));

      expect(api.get).toHaveBeenCalledWith('tenant/widgets/wgt-1');
      expect(result).toEqual(response);
    });
  });

  describe('create', () => {
    it('calls api.post with tenant/widgets and payload', async () => {
      const payload: CreateWidgetInstancePayload = { name: 'New Widget' };
      const response = { data: { id: 'wgt-2', name: 'New Widget' } };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(service.create(payload));

      expect(api.post).toHaveBeenCalledWith('tenant/widgets', payload);
      expect(result).toEqual(response);
    });
  });

  describe('update', () => {
    it('calls api.put with tenant/widgets/{id} and payload', async () => {
      const payload: UpdateWidgetInstancePayload = { name: 'Updated' };
      const response = { data: { id: 'wgt-1', name: 'Updated' } };
      api.put.mockReturnValue(of(response));

      const result = await firstValueFrom(service.update('wgt-1', payload));

      expect(api.put).toHaveBeenCalledWith('tenant/widgets/wgt-1', payload);
      expect(result).toEqual(response);
    });
  });

  describe('delete', () => {
    it('calls api.delete with tenant/widgets/{id}', async () => {
      const response = { data: undefined };
      api.delete.mockReturnValue(of(response));

      const result = await firstValueFrom(service.delete('wgt-1'));

      expect(api.delete).toHaveBeenCalledWith('tenant/widgets/wgt-1');
      expect(result).toEqual(response);
    });
  });

  describe('getSnippet', () => {
    it('calls api.get with tenant/widgets/{id}/snippet', async () => {
      const response = { data: { snippet: '<script>…</script>' } };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getSnippet('wgt-1'));

      expect(api.get).toHaveBeenCalledWith('tenant/widgets/wgt-1/snippet');
      expect(result).toEqual(response);
    });
  });
});
