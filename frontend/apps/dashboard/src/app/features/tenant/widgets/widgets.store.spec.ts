import { TestBed } from '@angular/core/testing';
import { of, Subject, throwError } from 'rxjs';
import { provideZonelessChangeDetection } from '@angular/core';
import { WidgetsStore } from './widgets.store';
import { WidgetApiService } from './widget-api.service';
import {
  WidgetInstance,
  CreateWidgetInstancePayload,
  UpdateWidgetInstancePayload,
} from '../../../core/api/widget.models';

describe('WidgetsStore', () => {
  let mockApi: {
    list: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    create: ReturnType<typeof vi.fn>;
    update: ReturnType<typeof vi.fn>;
    delete: ReturnType<typeof vi.fn>;
    getSnippet: ReturnType<typeof vi.fn>;
  };

  const mockInstances: WidgetInstance[] = [
    {
      id: 'wgt-1',
      publicId: 'wgt_abc123',
      name: 'Support Widget',
      displayName: 'Support Chat',
      primaryColor: '#0066FF',
      welcomeMessage: 'Hello! How can we help?',
      position: 'bottom-right',
      theme: 'light',
      enabled: true,
      allowedDomains: ['example.com'],
      createdAt: '2026-07-01T00:00:00Z',
      updatedAt: '2026-07-10T00:00:00Z',
    },
    {
      id: 'wgt-2',
      publicId: 'wgt_def456',
      name: 'Sales Widget',
      displayName: '',
      primaryColor: '#00AA55',
      welcomeMessage: '',
      position: 'bottom-left',
      theme: 'dark',
      enabled: false,
      allowedDomains: [],
      createdAt: '2026-07-05T00:00:00Z',
      updatedAt: '2026-07-12T00:00:00Z',
    },
  ];

  function configureStore() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        WidgetsStore,
        { provide: WidgetApiService, useValue: mockApi },
      ],
    });
    return TestBed.inject(WidgetsStore);
  }

  beforeEach(() => {
    mockApi = {
      list: vi.fn(),
      get: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
      getSnippet: vi.fn().mockReturnValue(of({ data: { snippet: '' } })),
    };
  });

  it('initializes with default state', () => {
    const listSub = new Subject();
    mockApi.list.mockReturnValue(listSub);
    const store = configureStore();

    expect(store.instances()).toEqual([]);
    expect(store.selectedId()).toBeNull();
    expect(store.formState()).toBeNull();
    expect(store.snippet()).toBeNull();
    expect(store.loading()).toBe(true);
    expect(store.saving()).toBe(false);
    expect(store.error()).toBeNull();
  });

  it('loads instances on init', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    configureStore();

    TestBed.flushEffects();

    expect(mockApi.list).toHaveBeenCalledOnce();
  });

  it('populates instances after successful loadList', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    const store = configureStore();
    TestBed.flushEffects();

    expect(store.loading()).toBe(false);
    expect(store.instances()).toEqual(mockInstances);
    expect(store.hasInstances()).toBe(true);
    expect(store.error()).toBeNull();
  });

  it('handles error on loadList', () => {
    const error = { message: 'Failed to load', code: 'ERR', status: 500 };
    mockApi.list.mockReturnValue(throwError(() => error));
    const store = configureStore();

    expect(store.loading()).toBe(false);
    expect(store.error()).toBe('Failed to load');
  });

  it('selects an instance and loads its snippet', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    mockApi.getSnippet.mockReturnValue(of({ data: { snippet: '<script>…</script>' } }));
    const store = configureStore();
    TestBed.flushEffects();

    store.selectInstance('wgt-1');

    expect(store.selectedId()).toBe('wgt-1');
    expect(store.formState()).toEqual(mockInstances[0]);
    expect(store.snippet()).toBe('<script>…</script>');
    expect(mockApi.getSnippet).toHaveBeenCalledWith('wgt-1');
  });

  it('selects null clears state', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    const store = configureStore();
    TestBed.flushEffects();

    store.selectInstance('wgt-1');
    store.selectInstance(null);

    expect(store.selectedId()).toBeNull();
    expect(store.formState()).toBeNull();
    expect(store.snippet()).toBeNull();
  });

  it('updates form state via updateFormState', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    const store = configureStore();
    TestBed.flushEffects();

    store.selectInstance('wgt-1');
    store.updateFormState({ displayName: 'Updated Chat' });

    expect(store.formState()?.displayName).toBe('Updated Chat');
    expect(store.formState()?.name).toBe('Support Widget');
  });

  it('creates an instance', () => {
    mockApi.list.mockReturnValue(of({ data: [] }));
    mockApi.create.mockReturnValue(of({ data: mockInstances[0] }));
    const store = configureStore();
    TestBed.flushEffects();

    const payload: CreateWidgetInstancePayload = { name: 'Support Widget' };
    store.createInstance(payload);

    expect(store.saving()).toBe(false);
    expect(store.instances()).toContainEqual(mockInstances[0]);
    expect(mockApi.create).toHaveBeenCalledWith(payload);
  });

  it('updates an instance', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    mockApi.getSnippet.mockReturnValue(of({ data: { snippet: '' } }));
    const updated = { ...mockInstances[0], displayName: 'Updated Name' };
    mockApi.update.mockReturnValue(of({ data: updated }));
    const store = configureStore();
    TestBed.flushEffects();

    const payload: UpdateWidgetInstancePayload = { displayName: 'Updated Name' };
    store.updateInstance('wgt-1', payload);

    expect(store.saving()).toBe(false);
    expect(store.instances()[0].displayName).toBe('Updated Name');
    expect(mockApi.update).toHaveBeenCalledWith('wgt-1', payload);
  });

  it('deletes an instance', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    mockApi.delete.mockReturnValue(of({ data: undefined }));
    const store = configureStore();
    TestBed.flushEffects();

    store.deleteInstance('wgt-1');

    expect(store.saving()).toBe(false);
    expect(store.instances().length).toBe(1);
    expect(store.instances()[0].id).toBe('wgt-2');
    expect(mockApi.delete).toHaveBeenCalledWith('wgt-1');
  });

  it('handles error on createInstance', () => {
    mockApi.list.mockReturnValue(new Subject());
    mockApi.create.mockReturnValue(new Subject());
    const store = configureStore();
    TestBed.flushEffects();

    const error = { message: 'Validation failed', code: 'ERR', status: 422 };
    const createSubject = new Subject<{ data: WidgetInstance }>();
    mockApi.create.mockReturnValue(createSubject);
    store.createInstance({ name: 'Test' });
    createSubject.error(error);

    expect(store.saving()).toBe(false);
    expect(store.error()).toBe('Validation failed');
  });

  it('computed selectedInstance returns correct instance', () => {
    mockApi.list.mockReturnValue(of({ data: mockInstances }));
    const store = configureStore();
    TestBed.flushEffects();

    store.selectInstance('wgt-1');
    expect(store.selectedInstance()?.id).toBe('wgt-1');
    expect(store.selectedInstance()?.name).toBe('Support Widget');
  });

  it('computed hasInstances reflects list state', () => {
    mockApi.list.mockReturnValue(of({ data: [] }));
    const store = configureStore();
    TestBed.flushEffects();

    expect(store.hasInstances()).toBe(false);
  });
});
