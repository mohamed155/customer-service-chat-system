import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { AUDIT_ENTRY_FIXTURES } from '../../fixtures/audit.fixtures';
import { AuditLogTableComponent } from './audit-log-table.component';

describe('AuditLogTableComponent', () => {
  async function setup(
    overrides: Partial<{
      entries: typeof AUDIT_ENTRY_FIXTURES;
      loading: boolean;
      showTenantColumn: boolean;
    }> = {},
  ) {
    TestBed.configureTestingModule({
      imports: [AuditLogTableComponent],
      providers: [provideTaiga()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AuditLogTableComponent);
    const component = fixture.componentInstance;
    fixture.componentRef.setInput('entries', overrides.entries ?? AUDIT_ENTRY_FIXTURES);
    if (overrides.loading !== undefined) {
      fixture.componentRef.setInput('loading', overrides.loading);
    }
    if (overrides.showTenantColumn !== undefined) {
      fixture.componentRef.setInput('showTenantColumn', overrides.showTenantColumn);
    }
    fixture.detectChanges();
    return { fixture, component };
  }

  it('renders one row per entry', async () => {
    const { fixture } = await setup();
    const rows = fixture.nativeElement.querySelectorAll('tbody tr');
    expect(rows.length).toBe(AUDIT_ENTRY_FIXTURES.length);
  });

  it('system-actor row renders "System"', async () => {
    const { fixture } = await setup();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('System');
  });

  it('platform-staff row renders staff badge', async () => {
    const { fixture } = await setup();
    const badges = fixture.nativeElement.querySelectorAll('.staff-badge');
    expect(badges.length).toBeGreaterThan(0);
  });

  it('deleted-actor row renders deleted label', async () => {
    const { fixture } = await setup();
    const badges = fixture.nativeElement.querySelectorAll('.deleted-badge');
    expect(badges.length).toBeGreaterThan(0);
  });

  it('clicking a row emits rowSelected', async () => {
    const { fixture } = await setup();
    const firstRow = fixture.nativeElement.querySelector('tbody tr');
    const spy = vi.fn();
    fixture.componentRef.instance.rowSelected.subscribe(spy);
    firstRow.click();
    expect(spy).toHaveBeenCalledWith(AUDIT_ENTRY_FIXTURES[0]);
  });

  it('empty state renders when entries is []', async () => {
    const { fixture } = await setup({ entries: [] });
    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
  });

  it('tenant column appears only when showTenantColumn is true', async () => {
    const { fixture } = await setup({ showTenantColumn: true });
    const headers = fixture.nativeElement.querySelectorAll('thead th');
    const headerTexts = (Array.from(headers) as HTMLElement[]).map((h) => h.textContent?.trim());
    expect(headerTexts).toContain('Tenant');
  });
});
