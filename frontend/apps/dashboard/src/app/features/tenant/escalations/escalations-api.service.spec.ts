import { HttpParams } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of, throwError } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { EscalationsApiService } from './escalations-api.service';

describe('EscalationsApiService', () => {
  let service: EscalationsApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), post: vi.fn() };
    TestBed.configureTestingModule({
      providers: [EscalationsApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(EscalationsApiService);
  });

  describe('listQueue', () => {
    it('maps the { data, pagination } wire envelope to a paginated response', async () => {
      const entry = { escalation: { id: 'esc-1' } };
      api.get.mockReturnValue(
        of({
          data: { data: [entry], pagination: { next_cursor: 'next-1', has_more: true } },
          requestId: 'req-1',
        }),
      );

      const result = await firstValueFrom(service.listQueue());

      expect(result).toEqual({
        data: { items: [entry], nextCursor: 'next-1', hasMore: true },
        requestId: 'req-1',
      });
    });

    it('passes cursor and limit as query params', async () => {
      api.get.mockReturnValue(
        of({ data: { data: [], pagination: { next_cursor: null, has_more: false } } }),
      );

      await firstValueFrom(service.listQueue({ cursor: 'prev-cursor', limit: 20 }));

      const [path, params] = api.get.mock.calls[0] as [string, HttpParams];
      expect(path).toBe('tenant/escalations/queue');
      expect(params.get('cursor')).toBe('prev-cursor');
      expect(params.get('limit')).toBe('20');
    });
  });

  describe('claim', () => {
    it('sends POST with escalation id and empty body', async () => {
      api.post.mockReturnValue(of({ data: undefined }));

      const result = await firstValueFrom(service.claim('esc-1'));

      expect(api.post).toHaveBeenCalledWith('tenant/escalations/esc-1/claim', {});
      expect(result.data).toBeUndefined();
    });

    it('propagates 409 Conflict', async () => {
      const conflict = { code: 'conflict', message: 'Already claimed', status: 409 };
      api.post.mockReturnValue(throwError(() => conflict));

      await expect(firstValueFrom(service.claim('esc-1'))).rejects.toEqual(conflict);
    });
  });
});
