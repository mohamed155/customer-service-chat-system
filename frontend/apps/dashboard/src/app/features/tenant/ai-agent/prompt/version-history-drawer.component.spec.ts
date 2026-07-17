import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Subject, of } from 'rxjs';
import {
  PromptVersionDetail,
  PromptVersionListItem,
  PromptVersionListResponse,
} from '../../../../core/api/ai-agent.models';
import { PromptApiService } from './prompt-api.service';
import { PromptStore } from './prompt.store';
import { VersionHistoryDrawerComponent } from './version-history-drawer.component';

describe('VersionHistoryDrawerComponent', () => {
  let mockApi: {
    getPrompt: ReturnType<typeof vi.fn>;
    savePrompt: ReturnType<typeof vi.fn>;
    listVersions: ReturnType<typeof vi.fn>;
    getVersion: ReturnType<typeof vi.fn>;
    restoreVersion: ReturnType<typeof vi.fn>;
  };

  const mockItems: PromptVersionListItem[] = [
    {
      versionNumber: 5,
      contentPreview: 'You are {{agent_name}} for {{tenant_name}}.',
      changeNote: 'Tightened refund wording',
      restoredFrom: null,
      createdAt: '2026-07-16T12:00:00Z',
      createdBy: 'Dana Ops',
      isActive: true,
    },
    {
      versionNumber: 4,
      contentPreview: 'You are {{agent_name}} for {{tenant_name}}.',
      changeNote: 'Added greeting',
      restoredFrom: 2,
      createdAt: '2026-07-15T10:00:00Z',
      createdBy: 'Sam Admin',
      isActive: false,
    },
  ];

  const mockDetail: PromptVersionDetail = {
    versionNumber: 4,
    content: 'You are {{agent_name}} for {{tenant_name}}.\nBe helpful.',
    changeNote: 'Added greeting',
    restoredFrom: 2,
    createdAt: '2026-07-15T10:00:00Z',
    createdBy: 'Sam Admin',
    isActive: false,
  };

  @Component({
    standalone: true,
    imports: [VersionHistoryDrawerComponent],
    template: ` <app-version-history-drawer [open]="open()" (closed)="open.set(false)" /> `,
    changeDetection: ChangeDetectionStrategy.OnPush,
  })
  class HostComponent {
    readonly open = signal(true);
  }

  beforeEach(() => {
    mockApi = {
      getPrompt: vi.fn(),
      savePrompt: vi.fn(),
      listVersions: vi.fn(),
      getVersion: vi.fn(),
      restoreVersion: vi.fn(),
    };
  });

  async function setup() {
    const promptSubject = new Subject<{ data: unknown }>();
    mockApi.getPrompt.mockReturnValue(promptSubject);

    TestBed.configureTestingModule({
      imports: [HostComponent],
      providers: [
        provideZonelessChangeDetection(),
        PromptStore,
        { provide: PromptApiService, useValue: mockApi },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(HostComponent);
    fixture.detectChanges();

    // Resolve onInit load
    promptSubject.next({
      data: {
        prompt: {
          exists: true,
          activeVersion: 5,
          content: 'You are {{agent_name}}.',
          updatedAt: '2026-07-16T10:12:00Z',
          updatedBy: 'Dana Ops',
        },
        variables: [],
        limits: { maxContentLength: 8000, maxChangeNoteLength: 500 },
      },
    });
    promptSubject.complete();

    // Load history
    const store = TestBed.inject(PromptStore);
    const listSubject = new Subject<{ data: PromptVersionListResponse }>();
    mockApi.listVersions.mockReturnValue(listSubject);
    store.loadHistory();
    listSubject.next({ data: { items: mockItems, hasMore: true } });
    listSubject.complete();
    fixture.detectChanges();

    return fixture;
  }

  it('renders version list items', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.version-row')).toBeTruthy();
    });
    const rows = fixture.nativeElement.querySelectorAll('.version-row');
    expect(rows.length).toBe(2);
  });

  it('shows active badge for active version', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.active-badge')).toBeTruthy();
    });
  });

  it('shows restored-from badge when applicable', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.restored-badge')).toBeTruthy();
    });
  });

  it('shows load more button when hasMore is true', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const btn = fixture.nativeElement.querySelector('.load-more-wrap app-button');
      expect(btn).toBeTruthy();
    });
  });

  it('selects version on row click and shows detail view', async () => {
    const fixture = await setup();
    mockApi.getVersion.mockReturnValue(of({ data: mockDetail }));

    await vi.waitFor(() => {
      fixture.detectChanges();
      const row = fixture.nativeElement.querySelector('.version-row');
      if (row) row.click();
    });
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.detail-view')).toBeTruthy();
    });
  });

  it('shows restore confirm pattern', async () => {
    const fixture = await setup();
    mockApi.getVersion.mockReturnValue(of({ data: mockDetail }));

    await vi.waitFor(() => {
      fixture.detectChanges();
      const row = fixture.nativeElement.querySelectorAll('.version-row')[1];
      if (row) row.click();
    });
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const restoreBtn = fixture.nativeElement.querySelector('.detail-view app-button button');
      if (restoreBtn) (restoreBtn as HTMLElement).click();
    });
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.confirm-bar')).toBeTruthy();
    });
  });

  it('cancels restore confirm', async () => {
    const fixture = await setup();
    mockApi.getVersion.mockReturnValue(of({ data: mockDetail }));

    await vi.waitFor(() => {
      fixture.detectChanges();
      const row = fixture.nativeElement.querySelectorAll('.version-row')[1];
      if (row) row.click();
    });
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const restoreBtn = fixture.nativeElement.querySelector('.detail-view app-button button');
      if (restoreBtn) (restoreBtn as HTMLElement).click();
    });
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.confirm-bar')).toBeTruthy();
      const cancelBtn = fixture.nativeElement.querySelector(
        '.confirm-bar .confirm-actions app-button:last-child button',
      );
      if (cancelBtn) (cancelBtn as HTMLElement).click();
    });
    TestBed.flushEffects();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.confirm-bar')).toBeFalsy();
    });
  });

  it('shows diff section with added and removed lines when a version is selected', async () => {
    const fixture = await setup();
    mockApi.getVersion.mockReturnValue(of({ data: mockDetail }));

    // Select version 4
    await vi.waitFor(() => {
      fixture.detectChanges();
      const row = fixture.nativeElement.querySelectorAll('.version-row')[1];
      if (row) row.click();
    });
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      const diffSection = fixture.nativeElement.querySelector('.diff-section');
      expect(diffSection).toBeTruthy();
      const addedLines = diffSection.querySelectorAll('.diff-added');
      const removedLines = diffSection.querySelectorAll('.diff-removed');
      // selected has 2 lines, active has 1, so diff: 2 removed + 1 added
      expect(removedLines.length).toBe(2);
      expect(addedLines.length).toBe(1);
    });
  });
});
