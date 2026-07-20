import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { AUDIT_ENTRY_FIXTURES } from '../../../shared/fixtures/audit.fixtures';
import { PlatformAuditLogsApiService } from './platform-audit-logs-api.service';
import { PlatformAuditLogsStore } from './platform-audit-logs.store';

const MOCK_LIST = {
  data: AUDIT_ENTRY_FIXTURES,
  pagination: { nextCursor: null, hasMore: false },
};

describe('PlatformAuditLogsStore', () => {
  const list = vi.fn();

  beforeEach(() => {
    list.mockReset();
    list.mockReturnValue(of({ data: MOCK_LIST }));
    TestBed.configureTestingModule({
      providers: [
        PlatformAuditLogsStore,
        { provide: PlatformAuditLogsApiService, useValue: { list } },
      ],
    });
  });

  it('load() populates entries and clears loading', async () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.entries().length).toBe(AUDIT_ENTRY_FIXTURES.length);
      expect(store.loading()).toBe(false);
    });
  });

  it('loadMore() appends entries', async () => {
    const page1 = {
      data: AUDIT_ENTRY_FIXTURES.slice(0, 2),
      pagination: { nextCursor: 'cursor2', hasMore: true },
    };
    const page2 = {
      data: AUDIT_ENTRY_FIXTURES.slice(2, 3),
      pagination: { nextCursor: null, hasMore: false },
    };
    list.mockReturnValue(of({ data: page1 }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    await vi.waitFor(() => expect(store.entries().length).toBe(2));
    expect(store.nextCursor()).toBe('cursor2');
    list.mockReturnValue(of({ data: page2 }));
    store.loadMore();
    await vi.waitFor(() => expect(store.entries().length).toBe(3));
  });

  it("setCategory('all') sends no category", () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    list.mockClear();
    store.setCategory('all');
    expect(list).toHaveBeenCalledWith(expect.not.objectContaining({ category: expect.anything() }));
  });

  it("setCategory('members') sends category: 'members'", () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    list.mockClear();
    store.setCategory('members');
    expect(list).toHaveBeenCalledWith(expect.objectContaining({ category: 'members' }));
  });

  it('setDateRange with from > to sets error and issues no request', () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    list.mockClear();
    store.setDateRange('2026-03-12', '2026-03-10');
    expect(store.error()).toBe('From date must be on or before To date');
    expect(list).not.toHaveBeenCalled();
  });

  it('openEntry/closeDrawer toggle selected entry and drawer', () => {
    const store = TestBed.inject(PlatformAuditLogsStore);
    const entry = AUDIT_ENTRY_FIXTURES[0];
    store.openEntry(entry);
    expect(store.selectedEntry()).toBe(entry);
    expect(store.drawerOpen()).toBe(true);
    store.closeDrawer();
    expect(store.drawerOpen()).toBe(false);
  });

  it('API error sets error and clears loading', async () => {
    list.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(PlatformAuditLogsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });

  it('setTenant sends tenant_id param', () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    list.mockClear();
    store.setTenant('tnt-acme');
    expect(list).toHaveBeenCalledWith(expect.objectContaining({ tenantId: 'tnt-acme' }));
  });

  it('setTenant(null) omits tenant_id param', () => {
    list.mockReturnValue(of({ data: MOCK_LIST }));
    const store = TestBed.inject(PlatformAuditLogsStore);
    list.mockClear();
    store.setTenant(null);
    expect(list).toHaveBeenCalledWith(expect.not.objectContaining({ tenantId: expect.anything() }));
  });
});
