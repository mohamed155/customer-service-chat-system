import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of, throwError } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { EscalationsApiService } from './escalations-api.service';

describe('EscalationsApiService', () => {
  let service: EscalationsApiService;
  let api: {
    list: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { list: vi.fn(), post: vi.fn() };
    TestBed.configureTestingModule({
      providers: [EscalationsApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(EscalationsApiService);
  });

  describe('listQueue', () => {
    it('passes query params through with no mapper', async () => {
      const response = {
        data: { items: [], nextCursor: null, hasMore: false },
      };
      api.list.mockReturnValue(of(response));

      const result = await firstValueFrom(service.listQueue({ cursor: 'cursor-abc', limit: 20 }));

      expect(api.list).toHaveBeenCalledWith('tenant/escalations/queue', {
        cursor: 'cursor-abc',
        limit: 20,
      });
      expect(result).toEqual(response);
    });

    it('passes cursor through to the API', async () => {
      api.list.mockReturnValue(of({ data: { items: [], nextCursor: null, hasMore: false } }));

      await firstValueFrom(service.listQueue({ cursor: 'prev-cursor' }));

      const [, query] = api.list.mock.calls[0];
      expect(query.cursor).toBe('prev-cursor');
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
