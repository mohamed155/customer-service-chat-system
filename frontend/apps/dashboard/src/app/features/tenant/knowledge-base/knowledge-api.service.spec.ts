import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ApiService } from '../../../core/api/api.service';
import { KnowledgeApiService } from './knowledge-api.service';

describe('KnowledgeApiService', () => {
  let service: KnowledgeApiService;
  let api: {
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
    delete: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { get: vi.fn(), post: vi.fn(), patch: vi.fn(), delete: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        KnowledgeApiService,
        { provide: ApiService, useValue: api },
        {
          provide: APP_CONFIG,
          useValue: { apiBaseUrl: '/api/v1' },
        },
      ],
    });
    service = TestBed.inject(KnowledgeApiService);
  });

  describe('listItems', () => {
    it('calls api.get with tenant/knowledge/items and query params', async () => {
      const response = { data: { items: [], hasMore: false, nextCursor: null } };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(
        service.listItems({ type: 'article', status: 'published' }, 'cursor-123'),
      );

      expect(api.get).toHaveBeenCalledWith('tenant/knowledge/items', expect.any(Object));
      const params = api.get.mock.calls[0][1];
      expect(params.get('type')).toBe('article');
      expect(params.get('status')).toBe('published');
      expect(params.get('before')).toBe('cursor-123');
      expect(result).toEqual(response);
    });
  });

  describe('getItem', () => {
    it('calls api.get with tenant/knowledge/items/{id}', async () => {
      const response = { data: { id: 'item-1', title: 'Test' } };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.getItem('item-1'));

      expect(api.get).toHaveBeenCalledWith('tenant/knowledge/items/item-1');
      expect(result).toEqual(response);
    });
  });

  describe('createItem', () => {
    it('calls api.post with tenant/knowledge/items and payload', async () => {
      const payload = { title: 'New Item', itemType: 'article' as const };
      const response = { data: { id: 'item-2', title: 'New Item', itemType: 'article' } };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(service.createItem(payload));

      expect(api.post).toHaveBeenCalledWith('tenant/knowledge/items', payload);
      expect(result).toEqual(response);
    });
  });

  describe('updateItem', () => {
    it('calls api.patch with tenant/knowledge/items/{id} and payload', async () => {
      const payload = { title: 'Updated Title' };
      const response = { data: { id: 'item-1', title: 'Updated Title' } };
      api.patch.mockReturnValue(of(response));

      const result = await firstValueFrom(service.updateItem('item-1', payload));

      expect(api.patch).toHaveBeenCalledWith('tenant/knowledge/items/item-1', payload);
      expect(result).toEqual(response);
    });
  });

  describe('setStatus', () => {
    it('calls api.post with tenant/knowledge/items/{id}/status and payload', async () => {
      const payload = { status: 'published' as const };
      const response = {
        data: {
          id: 'item-1',
          status: 'published' as const,
          changed: true,
          updatedAt: '2025-01-01T00:00:00Z',
        },
      };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(service.setStatus('item-1', payload));

      expect(api.post).toHaveBeenCalledWith('tenant/knowledge/items/item-1/status', payload);
      expect(result).toEqual(response);
    });
  });

  describe('listCategories', () => {
    it('calls api.get with tenant/knowledge/categories', async () => {
      const response = { data: [{ id: 'cat-1', name: 'General', itemCount: 0 }] };
      api.get.mockReturnValue(of(response));

      const result = await firstValueFrom(service.listCategories());

      expect(api.get).toHaveBeenCalledWith('tenant/knowledge/categories');
      expect(result).toEqual(response);
    });
  });

  describe('createCategory', () => {
    it('calls api.post with tenant/knowledge/categories and payload', async () => {
      const payload = { name: 'New Category' };
      const response = { data: { id: 'cat-2', name: 'New Category', itemCount: 0 } };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(service.createCategory(payload));

      expect(api.post).toHaveBeenCalledWith('tenant/knowledge/categories', payload);
      expect(result).toEqual(response);
    });
  });

  describe('renameCategory', () => {
    it('calls api.patch with tenant/knowledge/categories/{id} and payload', async () => {
      const payload = { name: 'Renamed Category' };
      const response = { data: { id: 'cat-1', name: 'Renamed Category', itemCount: 5 } };
      api.patch.mockReturnValue(of(response));

      const result = await firstValueFrom(service.renameCategory('cat-1', payload));

      expect(api.patch).toHaveBeenCalledWith('tenant/knowledge/categories/cat-1', payload);
      expect(result).toEqual(response);
    });
  });

  describe('deleteCategory', () => {
    it('calls api.delete with tenant/knowledge/categories/{id}', async () => {
      const response = { data: undefined };
      api.delete.mockReturnValue(of(response));

      const result = await firstValueFrom(service.deleteCategory('cat-1'));

      expect(api.delete).toHaveBeenCalledWith('tenant/knowledge/categories/cat-1');
      expect(result).toEqual(response);
    });
  });

  describe('uploadDocument', () => {
    it('posts FormData to tenant/knowledge/documents', async () => {
      const formData = new FormData();
      formData.append('file', new Blob(['test']), 'test.pdf');
      const response = {
        data: {
          id: 'doc-1',
          itemType: 'document' as const,
          title: 'Test',
          status: 'draft' as const,
          categoryId: null,
          categoryName: null,
          createdByDisplay: 'Alice',
          createdAt: '2026-07-01T00:00:00Z',
          updatedAt: '2026-07-01T00:00:00Z',
          tags: [],
          body: null,
          source: 'uploaded' as const,
          createdByUserId: null,
          document: {
            originalFilename: 'test.pdf',
            contentType: 'application/pdf',
            sizeBytes: 4,
            createdAt: '2026-07-01T00:00:00Z',
          },
        },
      };
      api.post.mockReturnValue(of(response));

      const result = await firstValueFrom(service.uploadDocument(formData));

      expect(api.post).toHaveBeenCalledWith('tenant/knowledge/documents', formData);
      expect(result).toEqual(response);
    });
  });

  describe('fileDownloadUrl', () => {
    it('constructs download URL from apiBaseUrl', () => {
      const url = service.fileDownloadUrl('doc-1');
      expect(url).toBe('/api/v1/tenant/knowledge/items/doc-1/file');
    });
  });
});
