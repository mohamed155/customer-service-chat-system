import { TestBed } from '@angular/core/testing';
import { of, Subject } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { KnowledgeStore } from './knowledge.store';
import { KnowledgeApiService } from './knowledge-api.service';
import {
  CreateItemPayload,
  KnowledgeItemDetail,
  KnowledgeItemSummary,
  KnowledgeCategory,
  SetStatusPayload,
  UpdateItemPayload,
} from '../../../core/api/knowledge.models';

describe('KnowledgeStore', () => {
  let mockApi: {
    listItems: ReturnType<typeof vi.fn>;
    getItem: ReturnType<typeof vi.fn>;
    createItem: ReturnType<typeof vi.fn>;
    updateItem: ReturnType<typeof vi.fn>;
    setStatus: ReturnType<typeof vi.fn>;
    listCategories: ReturnType<typeof vi.fn>;
    createCategory: ReturnType<typeof vi.fn>;
    renameCategory: ReturnType<typeof vi.fn>;
    deleteCategory: ReturnType<typeof vi.fn>;
  };

  const mockItems: KnowledgeItemSummary[] = [
    {
      id: 'kb-1',
      itemType: 'article',
      title: 'Returns policy',
      status: 'published',
      categoryId: 'cat-1',
      categoryName: 'Orders',
      createdByDisplay: 'Alice',
      createdAt: '2026-07-01T00:00:00Z',
      updatedAt: '2026-07-10T00:00:00Z',
      tags: [],
    },
    {
      id: 'kb-2',
      itemType: 'faq',
      title: 'Shipping FAQ',
      status: 'draft',
      categoryId: 'cat-2',
      categoryName: 'Shipping',
      createdByDisplay: 'Bob',
      createdAt: '2026-07-02T00:00:00Z',
      updatedAt: '2026-07-11T00:00:00Z',
      tags: ['shipping'],
    },
  ];

  const mockCategories: KnowledgeCategory[] = [
    { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
    { id: 'cat-2', name: 'Shipping', itemCount: 1, createdAt: '', updatedAt: '' },
  ];

  const mockDetail: KnowledgeItemDetail = {
    ...mockItems[0],
    body: '<p>Return rules</p>',
    source: 'authored',
    createdByUserId: 'user-1',
    document: null,
  };

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        KnowledgeStore,
        { provide: KnowledgeApiService, useValue: mockApi },
      ],
    });
    return TestBed.inject(KnowledgeStore);
  }

  beforeEach(() => {
    mockApi = {
      listItems: vi.fn(),
      getItem: vi.fn(),
      createItem: vi.fn(),
      updateItem: vi.fn(),
      setStatus: vi.fn(),
      listCategories: vi.fn(),
      createCategory: vi.fn(),
      renameCategory: vi.fn(),
      deleteCategory: vi.fn(),
    };
  });

  it('initializes with default state', () => {
    const itemsSub = new Subject();
    const catsSub = new Subject();
    mockApi.listItems.mockReturnValue(itemsSub);
    mockApi.listCategories.mockReturnValue(catsSub);
    const store = configureStore();

    expect(store.items()).toEqual([]);
    expect(store.selectedItem()).toBeNull();
    expect(store.categories()).toEqual([]);
    expect(store.filters()).toEqual({});
    expect(store.cursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
    expect(store.loading()).toBe(true);
    expect(store.saving()).toBe(false);
    expect(store.error()).toBeNull();
  });

  it('loads items and categories on init', () => {
    mockApi.listItems.mockReturnValue(
      of({ data: { items: mockItems, hasMore: false, nextCursor: null } }),
    );
    mockApi.listCategories.mockReturnValue(of({ data: mockCategories }));
    configureStore();

    TestBed.flushEffects();

    expect(mockApi.listItems).toHaveBeenCalledOnce();
    expect(mockApi.listCategories).toHaveBeenCalledOnce();
  });

  it('populates items after successful loadList', () => {
    mockApi.listItems.mockReturnValue(
      of({ data: { items: mockItems, hasMore: true, nextCursor: 'cursor-2' } }),
    );
    mockApi.listCategories.mockReturnValue(of({ data: mockCategories }));
    const store = configureStore();

    TestBed.flushEffects();

    expect(store.loading()).toBe(false);
    expect(store.items()).toEqual(mockItems);
    expect(store.hasMore()).toBe(true);
    expect(store.cursor()).toBe('cursor-2');
    expect(store.error()).toBeNull();
  });

  it('loads more items via loadMore', () => {
    mockApi.listItems.mockReturnValueOnce(
      of({ data: { items: mockItems.slice(0, 1), hasMore: true, nextCursor: 'cursor-2' } }),
    );
    mockApi.listCategories.mockReturnValue(of({ data: mockCategories }));
    const store = configureStore();
    TestBed.flushEffects();

    mockApi.listItems.mockReturnValue(
      of({ data: { items: mockItems.slice(1), hasMore: false, nextCursor: null } }),
    );
    store.loadMore();

    expect(store.items()).toEqual(mockItems);
  });

  it('sets filter and reloads list', () => {
    mockApi.listItems.mockReturnValue(
      of({ data: { items: mockItems, hasMore: false, nextCursor: null } }),
    );
    mockApi.listCategories.mockReturnValue(of({ data: mockCategories }));
    const store = configureStore();
    TestBed.flushEffects();

    store.setFilter({ type: 'article' });
    expect(store.filters()).toEqual({ type: 'article' });
    expect(store.cursor()).toBeNull();
  });

  it('loads item detail via loadItem', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.getItem.mockReturnValue(of({ data: mockDetail }));
    const store = configureStore();
    TestBed.flushEffects();

    store.loadItem('kb-1');
    expect(store.loading()).toBe(false);
    expect(store.selectedItem()).toEqual(mockDetail);
  });

  it('creates item via createItem', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.createItem.mockReturnValue(of({ data: mockDetail }));
    const store = configureStore();
    TestBed.flushEffects();

    const payload: CreateItemPayload = { title: 'Returns policy', itemType: 'article' };
    store.createItem(payload);
    expect(store.saving()).toBe(false);
    expect(store.selectedItem()).toEqual(mockDetail);
    expect(mockApi.createItem).toHaveBeenCalledWith(payload);
  });

  it('updates item via updateItem', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.updateItem.mockReturnValue(of({ data: { ...mockDetail, title: 'Updated' } }));
    const store = configureStore();
    TestBed.flushEffects();

    const payload: UpdateItemPayload = { title: 'Updated' };
    store.updateItem('kb-1', payload);
    expect(store.saving()).toBe(false);
    expect(store.selectedItem()?.title).toBe('Updated');
    expect(mockApi.updateItem).toHaveBeenCalledWith('kb-1', payload);
  });

  it('sets status with changed:true — updates items and selectedItem', () => {
    mockApi.listItems.mockReturnValue(
      of({ data: { items: mockItems, hasMore: false, nextCursor: null } }),
    );
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.getItem.mockReturnValue(of({ data: mockDetail }));
    mockApi.setStatus.mockReturnValue(
      of({
        data: {
          id: 'kb-1',
          status: 'archived' as const,
          changed: true,
          updatedAt: '2026-07-15T00:00:00Z',
        },
      }),
    );
    const store = configureStore();
    TestBed.flushEffects();
    expect(store.items().length).toBe(2);
    store.loadItem('kb-1');

    const payload: SetStatusPayload = { status: 'archived' };
    store.setStatus('kb-1', payload);

    expect(store.saving()).toBe(false);
    expect(store.selectedItem()?.status).toBe('archived');
    expect(store.items()[0].status).toBe('archived');
    expect(mockApi.setStatus).toHaveBeenCalledWith('kb-1', payload);
  });

  it('sets status with changed:false — no state update beyond saving', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.setStatus.mockReturnValue(
      of({ data: { id: 'kb-1', status: 'published' as const, changed: false, updatedAt: '' } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.setStatus('kb-1', { status: 'published' });
    expect(store.saving()).toBe(false);
    expect(store.error()).toBeNull();
  });

  it('handles error on setStatus', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.setStatus.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error = { message: 'Cannot archive', code: 'ERR', status: 422 };
    const setSubject = new Subject<{
      data: { id: string; status: string; changed: boolean; updatedAt: string };
    }>();
    mockApi.setStatus.mockReturnValue(setSubject);
    store.setStatus('kb-1', { status: 'archived' });
    setSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.error()).toBe('Cannot archive');
  });

  it('handles error on loadList', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error = { message: 'Network error', code: 'ERR', status: 500 };
    const listSubject = new Subject<{
      data: { items: KnowledgeItemSummary[]; hasMore: boolean; nextCursor: string | null };
    }>();
    mockApi.listItems.mockReturnValue(listSubject);
    store.loadList();
    listSubject.error(error);

    expect(store.loading()).toBe(false);
    expect(store.error()).toBe('Network error');
  });

  it('creates category via createCategory and refreshes list', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.createCategory.mockReturnValue(
      of({ data: { id: 'cat-3', name: 'Support', itemCount: 0, createdAt: '', updatedAt: '' } }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.createCategory('Support');
    expect(store.saving()).toBe(false);
    expect(mockApi.createCategory).toHaveBeenCalledWith({ name: 'Support' });
  });

  it('handles error on createCategory', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.createCategory.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error = { message: 'Duplicate name', code: 'ERR', status: 409 };
    const catSubject = new Subject<{
      data: { id: string; name: string; itemCount: number; createdAt: string; updatedAt: string };
    }>();
    mockApi.createCategory.mockReturnValue(catSubject);
    store.createCategory('Duplicate');
    catSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.error()).toBe('Duplicate name');
  });

  it('renames category via renameCategory and refreshes', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.renameCategory.mockReturnValue(
      of({
        data: { id: 'cat-1', name: 'Orders Renamed', itemCount: 1, createdAt: '', updatedAt: '' },
      }),
    );
    const store = configureStore();
    TestBed.flushEffects();

    store.renameCategory('cat-1', 'Orders Renamed');
    expect(store.saving()).toBe(false);
    expect(mockApi.renameCategory).toHaveBeenCalledWith('cat-1', { name: 'Orders Renamed' });
  });

  it('deletes category via deleteCategory and refreshes', () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    mockApi.deleteCategory.mockReturnValue(of({ data: undefined }));
    const store = configureStore();
    TestBed.flushEffects();

    store.deleteCategory('cat-1');
    expect(store.saving()).toBe(false);
    expect(mockApi.deleteCategory).toHaveBeenCalledWith('cat-1');
  });
});
