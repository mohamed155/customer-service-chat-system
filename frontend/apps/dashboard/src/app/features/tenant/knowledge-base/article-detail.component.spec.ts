import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, Subject } from 'rxjs';
import { ArticleDetailComponent } from './article-detail.component';
import { KnowledgeApiService } from './knowledge-api.service';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { KnowledgeItemDetail } from '../../../core/api/knowledge.models';

describe('ArticleDetailComponent', () => {
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
  let permissions: { has: ReturnType<typeof vi.fn> };

  function mockDetailWithStatus(status: KnowledgeItemDetail['status']): KnowledgeItemDetail {
    return {
      id: 'kb-1',
      itemType: 'article',
      title: 'Returns policy',
      status,
      categoryId: 'cat-1',
      categoryName: 'Orders',
      createdByDisplay: 'Alice',
      createdAt: '2026-07-01T00:00:00Z',
      updatedAt: '2026-07-10T00:00:00Z',
      tags: ['returns', 'policy'],
      body: '<p>Return rules for all orders.</p>',
      source: 'authored',
      createdByUserId: 'user-1',
      document: null,
    };
  }

  async function createFixture(detail: KnowledgeItemDetail) {
    mockApi.getItem = vi.fn(() => of({ data: detail }));
    await TestBed.configureTestingModule({
      imports: [ArticleDetailComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: convertToParamMap({ id: 'kb-1' }) } },
        },
        { provide: KnowledgeApiService, useValue: mockApi },
        { provide: PermissionsService, useValue: permissions },
      ],
    }).compileComponents();
    return TestBed.createComponent(ArticleDetailComponent);
  }

  beforeEach(() => {
    mockApi = {
      listItems: vi.fn(),
      getItem: vi.fn(),
      createItem: vi.fn(),
      updateItem: vi.fn(),
      setStatus: vi.fn(),
      listCategories: vi.fn(() => of({ data: [] })),
      createCategory: vi.fn(),
      renameCategory: vi.fn(),
      deleteCategory: vi.fn(),
    };
    mockApi.listItems.mockReturnValue(new Subject());
    permissions = { has: vi.fn() };
  });

  it('renders article detail', async () => {
    permissions.has.mockReturnValue(true);
    const fixture = await createFixture(mockDetailWithStatus('published'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Returns policy');
    });
  });

  describe('status actions for manage user', () => {
    beforeEach(() => {
      permissions.has.mockImplementation((p: string) => p === 'knowledge_base.manage');
    });

    it('shows Publish button when status is draft', async () => {
      const fixture = await createFixture(mockDetailWithStatus('draft'));
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        const btn = fixture.nativeElement.querySelector('button');
        expect(btn?.textContent).toContain('Publish');
      });
    });

    it('shows Archive button when status is published', async () => {
      const fixture = await createFixture(mockDetailWithStatus('published'));
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('Archive');
      });
    });

    it('shows Restore button when status is archived', async () => {
      const fixture = await createFixture(mockDetailWithStatus('archived'));
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('Restore');
      });
    });
  });

  it('hides status actions for view-only user', async () => {
    permissions.has.mockReturnValue(false);
    const fixture = await createFixture(mockDetailWithStatus('draft'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).not.toContain('Publish');
      expect(fixture.nativeElement.textContent).not.toContain('Archive');
      expect(fixture.nativeElement.textContent).not.toContain('Restore');
    });
  });

  it('surfaces server error on rejected publish', async () => {
    permissions.has.mockReturnValue(true);
    const errSubject = new Subject<{
      data: { id: string; status: string; changed: boolean; updatedAt: string };
    }>();
    mockApi.setStatus.mockReturnValue(errSubject);
    const fixture = await createFixture(mockDetailWithStatus('draft'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const btn = fixture.nativeElement.querySelector('button');
      expect(btn?.textContent).toContain('Publish');
    });

    btnClick(fixture, 'Publish');

    errSubject.error({ message: 'Cannot publish without body', code: 'ERR', status: 422 });
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Cannot publish without body');
    });
  });

  function btnClick(fixture: ReturnType<typeof TestBed.createComponent>, label: string) {
    const buttons = fixture.nativeElement.querySelectorAll('button');
    for (const btn of buttons) {
      if (btn.textContent?.trim().includes(label)) {
        btn.click();
        break;
      }
    }
  }
});
