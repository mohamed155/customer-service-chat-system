import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { TUI_EDITOR_EXTENSIONS } from '@taiga-ui/editor/common';
import { of, Subject } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { ArticleEditorComponent } from './article-editor.component';
import { KnowledgeApiService } from './knowledge-api.service';

describe('ArticleEditorComponent', () => {
  let mockApi: ReturnType<typeof createMockApi>;

  function createMockApi() {
    return {
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
  }

  const testProviders = [
    provideTaiga(),
    provideZonelessChangeDetection(),
    provideRouter([]),
    { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
    { provide: TUI_EDITOR_EXTENSIONS, useValue: [] },
  ];

  beforeEach(() => {
    mockApi = createMockApi();
    mockApi.listItems.mockReturnValue(new Subject());
  });

  async function createFixture() {
    await TestBed.configureTestingModule({
      imports: [ArticleEditorComponent],
      providers: [...testProviders, { provide: KnowledgeApiService, useValue: mockApi }],
    }).compileComponents();
    return TestBed.createComponent(ArticleEditorComponent);
  }

  it('renders create mode with correct heading', async () => {
    const fixture = await createFixture();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('New article');
  });

  it('shows title validation on save with empty title', async () => {
    const fixture = await createFixture();
    fixture.detectChanges();

    const saveBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Create'));
    (saveBtn as HTMLButtonElement).click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Title is required');
  });
});
