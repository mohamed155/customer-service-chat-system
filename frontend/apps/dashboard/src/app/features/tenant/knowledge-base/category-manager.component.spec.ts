import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { of, Subject } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { CategoryManagerComponent } from './category-manager.component';
import { KnowledgeApiService } from './knowledge-api.service';
import { KnowledgeStore } from './knowledge.store';
import { KnowledgeCategory } from '../../../core/api/knowledge.models';

describe('CategoryManagerComponent', () => {
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
    };
  }

  const testProviders = [
    provideTaiga(),
    provideZonelessChangeDetection(),
    provideMockStore(),
    { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
  ];

  let mockPermissions: { has: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    mockPermissions = { has: vi.fn(() => true) };
  });

  async function createFixture(mockPerms = mockPermissions) {
    await TestBed.configureTestingModule({
      imports: [CategoryManagerComponent],
      providers: [
        ...testProviders,
        KnowledgeStore,
        { provide: KnowledgeApiService, useValue: mockApi },
        { provide: PermissionsService, useValue: mockPerms },
      ],
    }).compileComponents();
    return TestBed.createComponent(CategoryManagerComponent);
  }

  beforeEach(() => {
    mockApi = createMockApi();
  });

  it('renders the dialog with title', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    const fixture = await createFixture();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-dialog-shell')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Manage categories');
  });

  it('shows empty state when no categories', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('No categories yet');
    });
  });

  it('shows add category button for manage users', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Add category');
    });
  });

  it('opens add form when add category is clicked', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const addBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.includes('Add category'));
      (addBtn as HTMLButtonElement)?.click();
      fixture.detectChanges();
    });

    expect(
      (fixture.nativeElement as HTMLElement).querySelector('[aria-label="New category name"]'),
    ).toBeTruthy();
  });

  it('calls createCategory when adding a category', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
    mockApi.createCategory.mockReturnValue(
      of({ data: { id: 'cat-3', name: 'Support', itemCount: 0, createdAt: '', updatedAt: '' } }),
    );
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const addBtn = Array.from(
        (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
      ).find((b) => b.textContent?.includes('Add category'));
      (addBtn as HTMLButtonElement)?.click();
      fixture.detectChanges();
    });

    const input = (fixture.nativeElement as HTMLElement).querySelector(
      '[aria-label="New category name"]',
    ) as HTMLInputElement;
    input.value = 'Support';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Add'));
    (saveBtn as HTMLButtonElement)?.click();
    fixture.detectChanges();

    expect(mockApi.createCategory).toHaveBeenCalledWith({ name: 'Support' });
  });

  it('shows delete confirmation with uncategorized note', async () => {
    const mockCats: KnowledgeCategory[] = [
      { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
    ];
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: mockCats }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const deleteBtns = (fixture.nativeElement as HTMLElement).querySelectorAll(
        '[aria-label="Delete category"]',
      );
      expect(deleteBtns.length).toBe(1);
      (deleteBtns[0] as HTMLButtonElement).click();
      fixture.detectChanges();
    });

    expect(fixture.nativeElement.textContent).toContain('Delete');
    expect(fixture.nativeElement.textContent).toContain(
      'Affected items become uncategorized rather than deleted',
    );
  });

  it('calls deleteCategory on confirm', async () => {
    const mockCats: KnowledgeCategory[] = [
      { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
    ];
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: mockCats }));
    mockApi.deleteCategory.mockReturnValue(of({ data: undefined }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const deleteBtns = (fixture.nativeElement as HTMLElement).querySelectorAll(
        '[aria-label="Delete category"]',
      );
      (deleteBtns[0] as HTMLButtonElement).click();
      fixture.detectChanges();
    });

    const confirmBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Delete');

    if (confirmBtn) {
      (confirmBtn as HTMLButtonElement).click();
      fixture.detectChanges();
    }

    expect(mockApi.deleteCategory).toHaveBeenCalledWith('cat-1');
  });

  it('emits close when close button is clicked', async () => {
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(new Subject());
    const fixture = await createFixture();
    fixture.detectChanges();

    const closeSpy = vi.fn();
    (fixture.componentInstance as CategoryManagerComponent).closed.subscribe(closeSpy);

    const closeBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Close'));
    (closeBtn as HTMLButtonElement)?.click();
    fixture.detectChanges();

    expect(closeSpy).toHaveBeenCalled();
  });

  it('calls renameCategory when renaming', async () => {
    const mockCats: KnowledgeCategory[] = [
      { id: 'cat-1', name: 'Orders', itemCount: 1, createdAt: '', updatedAt: '' },
    ];
    mockApi.listItems.mockReturnValue(new Subject());
    mockApi.listCategories.mockReturnValue(of({ data: mockCats }));
    mockApi.renameCategory.mockReturnValue(of({ data: mockCats[0] }));
    const fixture = await createFixture();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const editBtns = (fixture.nativeElement as HTMLElement).querySelectorAll(
        '[aria-label="Rename category"]',
      );
      expect(editBtns.length).toBe(1);
      (editBtns[0] as HTMLButtonElement).click();
      fixture.detectChanges();
    });

    const input = (fixture.nativeElement as HTMLElement).querySelector(
      '[aria-label="Category name"]',
    ) as HTMLInputElement;
    input.value = 'Orders Renamed';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Save'));
    (saveBtn as HTMLButtonElement)?.click();
    fixture.detectChanges();

    expect(mockApi.renameCategory).toHaveBeenCalledWith('cat-1', { name: 'Orders Renamed' });
  });
});
