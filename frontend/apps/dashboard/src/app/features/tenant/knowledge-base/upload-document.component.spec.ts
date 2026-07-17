import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { of } from 'rxjs';
import { APP_CONFIG } from '../../../core/config/app-config';
import { KnowledgeApiService } from './knowledge-api.service';
import { UploadDocumentComponent } from './upload-document.component';

describe('UploadDocumentComponent', () => {
  let mockApi: ReturnType<typeof createMockApi>;

  function createMockApi() {
    return {
      uploadDocument: vi.fn(),
      fileDownloadUrl: vi.fn(),
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

  beforeEach(() => {
    mockApi = createMockApi();
    mockApi.listCategories.mockReturnValue(of({ data: [] }));
  });

  async function createFixture() {
    await TestBed.configureTestingModule({
      imports: [UploadDocumentComponent],
      providers: [...testProviders, { provide: KnowledgeApiService, useValue: mockApi }],
    }).compileComponents();
    return TestBed.createComponent(UploadDocumentComponent);
  }

  it('renders dialog shell when open', async () => {
    const fixture = await createFixture();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Upload document');
    });
  });

  it('shows validation error for rejected file type', async () => {
    const fixture = await createFixture();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();

    const fileInput = fixture.nativeElement.querySelector('#file') as HTMLInputElement;
    expect(fileInput).toBeTruthy();

    const blob = new Blob(['test']);
    const file = new File([blob], 'test.exe', { type: 'application/x-msdownload' });
    Object.defineProperty(fileInput, 'files', { value: [file] });
    fileInput.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Invalid file type');
    });
  });

  it('shows validation error for oversized file', async () => {
    const fixture = await createFixture();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();

    const fileInput = fixture.nativeElement.querySelector('#file') as HTMLInputElement;

    const blob = new Blob(['x'.repeat(21 * 1024 * 1024)]);
    const file = new File([blob], 'large.pdf', { type: 'application/pdf' });
    Object.defineProperty(fileInput, 'files', { value: [file] });
    fileInput.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('exceeds 20 MB');
    });
  });

  it('auto-fills title from filename stem', async () => {
    const fixture = await createFixture();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();

    const fileInput = fixture.nativeElement.querySelector('#file') as HTMLInputElement;

    const blob = new Blob(['test content']);
    const file = new File([blob], 'my-report.pdf', { type: 'application/pdf' });
    Object.defineProperty(fileInput, 'files', { value: [file] });
    fileInput.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const titleInput = fixture.nativeElement.querySelector('#title') as HTMLInputElement;
      expect(titleInput.value).toBe('my-report');
    });
  });

  it('emits completed on upload success', async () => {
    const fixture = await createFixture();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();

    const createdItem = {
      id: 'doc-1',
      itemType: 'document' as const,
      title: 'My Report',
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
        originalFilename: 'my-report.pdf',
        contentType: 'application/pdf',
        sizeBytes: 12,
        createdAt: '2026-07-01T00:00:00Z',
      },
    };
    mockApi.uploadDocument.mockReturnValue(of({ data: createdItem }));

    const fileInput = fixture.nativeElement.querySelector('#file') as HTMLInputElement;
    const blob = new Blob(['test content']);
    const file = new File([blob], 'my-report.pdf', { type: 'application/pdf' });
    Object.defineProperty(fileInput, 'files', { value: [file] });
    fileInput.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    const completedSpy = vi.fn();
    fixture.componentRef.instance.completed.subscribe(completedSpy);

    const uploadBtn = fixture.nativeElement.querySelector('.btn-upload') as HTMLButtonElement;
    uploadBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      expect(completedSpy).toHaveBeenCalledWith(createdItem);
    });
  });
});
