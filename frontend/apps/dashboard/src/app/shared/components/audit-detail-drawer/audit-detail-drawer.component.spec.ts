import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { AUDIT_ENTRY_FIXTURES } from '../../fixtures/audit.fixtures';
import { AuditDetailDrawerComponent } from './audit-detail-drawer.component';

describe('AuditDetailDrawerComponent', () => {
  async function setup(
    overrides: { entry?: (typeof AUDIT_ENTRY_FIXTURES)[0] | null; open?: boolean } = {},
  ) {
    TestBed.configureTestingModule({
      imports: [AuditDetailDrawerComponent],
      providers: [provideTaiga()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AuditDetailDrawerComponent);
    fixture.componentRef.setInput('entry', overrides.entry ?? null);
    fixture.componentRef.setInput('open', overrides.open ?? true);
    fixture.detectChanges();
    return { fixture };
  }

  it('renders nothing when entry is null', async () => {
    const { fixture } = await setup({ entry: null, open: true });
    const text = fixture.nativeElement.textContent?.trim();
    expect(text).toBe('');
  });

  it('nested-details fixture renders as pretty-printed JSON', async () => {
    const { fixture } = await setup({ entry: AUDIT_ENTRY_FIXTURES[0], open: true });
    const pre = fixture.nativeElement.querySelector('pre');
    expect(pre).toBeTruthy();
    const parsed = JSON.parse(pre.textContent);
    expect(parsed).toEqual(AUDIT_ENTRY_FIXTURES[0].details);
  });

  it('close button emits closed', async () => {
    const { fixture } = await setup({ entry: AUDIT_ENTRY_FIXTURES[0], open: true });
    const spy = vi.fn();
    fixture.componentRef.instance.closed.subscribe(spy);
    const closeBtn = fixture.nativeElement.querySelector('.close-btn');
    closeBtn.click();
    expect(spy).toHaveBeenCalled();
  });
});
