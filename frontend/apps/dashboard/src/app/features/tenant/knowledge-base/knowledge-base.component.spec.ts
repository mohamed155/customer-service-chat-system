import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { of, Subject } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { KnowledgeCategory } from '../../../core/api/knowledge.models';
import { KnowledgeBaseComponent } from './knowledge-base.component';
import { KnowledgeApiService } from './knowledge-api.service';

describe('KnowledgeBaseComponent', () => {
  let mockApi: ReturnType<typeof createMockApi>;

  function createMockApi() {
    return {
      listItems: vi.fn(),
      getItem: vi.fn(),
      createItem: vi.fn(),
      updateItem: vi.fn(),
      setStatus: vi.fn(),
      listCategories: vi.fn(),
      createCategory: vi.fn(),
      renameCategory: vi.fn(),
      deleteCategory: vi.fn(),
      uploadDocument: vi.fn(),
      fileDownloadUrl: vi.fn(),
    };
  }

  const mockResponse = {
    data: {
      items: [
        {
          id: 'kb-1',
          itemType: 'article' as const,
          title: 'Returns policy',
          status: 'published' as const,
          categoryId: 'cat-1',
          categoryName: 'Orders',
          createdByDisplay: 'Alice',
          createdAt: '2026-07-01T00:00:00Z',
          updatedAt: '2026-07-10T00:00:00Z',
          tags: [],
        },
        {
          id: 'kb-2',
          itemType: 'faq' as const,
          title: 'Shipping FAQ',
          status: 'draft' as const,
          categoryId: 'cat-2',
          categoryName: 'Shipping',
          createdByDisplay: 'Bob',
          createdAt: '2026-07-02T00:00:00Z',
          updatedAt: '2026-07-11T00:00:00Z',
          tags: ['shipping'],
        },
      ],
      hasMore: false,
      nextCursor: null,
    },
  };

  const testProviders = [
    provideTaiga(),
    provideZonelessChangeDetection(),
    provideRouter([]),
    provideMockStore(),
    { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
  ];

  let mockPermissions: { has: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    mockApi = createMockApi();
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
    mockPermissions = { has: vi.fn(() => true) };
  });

  async function createFixture() {
    await TestBed.configureTestingModule({
      imports: [KnowledgeBaseComponent],
      providers: [
        ...testProviders,
        { provide: KnowledgeApiService, useValue: mockApi },
        { provide: PermissionsService, useValue: mockPermissions },
      ],
    }).compileComponents();
    return TestBed.createComponent(KnowledgeBaseComponent);
  }

  it('moves from loading to content', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Returns policy');
    });
  });

  it('shows empty state when no items', async () => {
    mockApi.listItems.mockReturnValue(
      of({ data: { items: [], hasMore: false, nextCursor: null } }),
    );
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('shows upload button for manage permission', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const btn = fixture.nativeElement.querySelector('.upload-btn');
      expect(btn).toBeTruthy();
      expect(btn.textContent).toContain('Upload');
    });
  });

  it('opens upload dialog on button click', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const btn = fixture.nativeElement.querySelector('.upload-btn');
      btn.click();
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Upload document');
    });
  });

  it('shows error state and retries', async () => {
    const subject = new Subject<typeof mockResponse>();
    mockApi.listItems.mockReturnValue(subject);
    const fixture = await createFixture();
    fixture.detectChanges();

    subject.error({ message: 'fail', code: 'ERR', status: 500 });
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });
  });

  it('shows category filter dropdown', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    mockApi.listCategories.mockReturnValue(
      of({
        data: [
          { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
        ] as KnowledgeCategory[],
      }),
    );
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const selects = (fixture.nativeElement as HTMLElement).querySelectorAll('select');
      const catSelect = Array.from(selects).find(
        (s) => (s as HTMLSelectElement).getAttribute('aria-label') === 'Category filter',
      );
      expect(catSelect).toBeTruthy();
      expect((catSelect as HTMLSelectElement).textContent).toContain('Orders');
    });
  });

  it('shows tag filter input', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const input = (fixture.nativeElement as HTMLElement).querySelector(
        '[aria-label="Tag filter"]',
      );
      expect(input).toBeTruthy();
    });
  });

  it('shows manage categories button for manage users', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const btn = (fixture.nativeElement as HTMLElement).querySelector('.manage-cats-btn');
      expect(btn).toBeTruthy();
      expect(btn?.textContent).toContain('Categories');
    });
  });

  it('category filter change calls setFilter', async () => {
    mockApi.listItems.mockReturnValue(of(mockResponse));
    mockApi.listCategories.mockReturnValue(
      of({
        data: [
          { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
        ] as KnowledgeCategory[],
      }),
    );
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const selects = (fixture.nativeElement as HTMLElement).querySelectorAll('select');
      const catSelect = Array.from(selects).find(
        (s) => (s as HTMLSelectElement).getAttribute('aria-label') === 'Category filter',
      ) as HTMLSelectElement;
      catSelect.value = 'cat-1';
      catSelect.dispatchEvent(new Event('change', { bubbles: true }));
      fixture.detectChanges();
    });

    expect(mockApi.listItems).toHaveBeenCalledTimes(2);
  });
});
