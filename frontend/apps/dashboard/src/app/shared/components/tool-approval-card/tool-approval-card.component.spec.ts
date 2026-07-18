import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ToolRequest } from '../../../core/api/tenant-api.models';
import { ToolApprovalCardComponent } from './tool-approval-card.component';

describe('ToolApprovalCardComponent', () => {
  const pendingRequest: ToolRequest = {
    id: 'tr-approve-1',
    toolName: 'update_customer_contact',
    toolSource: 'builtin',
    arguments: { field: 'email', value: 'new@example.com' },
    status: 'awaiting_approval',
    approvalRequired: true,
    chainIndex: 0,
    createdAt: '2026-07-18T10:00:00Z',
    expiresAt: '2026-07-18T10:05:00Z',
  };

  function createComponent(request: ToolRequest) {
    TestBed.configureTestingModule({
      imports: [ToolApprovalCardComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ToolApprovalCardComponent);
    fixture.componentRef.setInput('request', request);
    fixture.detectChanges();
    return fixture;
  }

  it('renders tool name and pending status', () => {
    const fixture = createComponent(pendingRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('update_customer_contact');
    expect(el.textContent).toContain('pending approval');
  });

  it('shows Approve and Deny buttons when awaiting_approval', () => {
    const fixture = createComponent(pendingRequest);
    const el = fixture.nativeElement as HTMLElement;
    const buttons = el.querySelectorAll('button');
    expect(buttons.length).toBe(2);
    expect(buttons[0].textContent).toContain('Approve');
    expect(buttons[1].textContent).toContain('Deny');
  });

  it('emits approve event when Approve is clicked', () => {
    const fixture = createComponent(pendingRequest);
    const spy = vi.fn();
    fixture.componentRef.instance.approve.subscribe(spy);
    const el = fixture.nativeElement as HTMLElement;
    const approveBtn = el.querySelector('.btn-approve') as HTMLButtonElement;
    approveBtn.click();
    expect(spy).toHaveBeenCalledWith('tr-approve-1');
  });

  it('emits deny event when Deny is clicked', () => {
    const fixture = createComponent(pendingRequest);
    const spy = vi.fn();
    fixture.componentRef.instance.deny.subscribe(spy);
    const el = fixture.nativeElement as HTMLElement;
    const denyBtn = el.querySelector('.btn-deny') as HTMLButtonElement;
    denyBtn.click();
    expect(spy).toHaveBeenCalledWith('tr-approve-1');
  });

  it('disables buttons and shows resolved state when status changes from awaiting_approval', () => {
    const resolvedRequest: ToolRequest = {
      ...pendingRequest,
      status: 'approved',
    };
    const fixture = createComponent(resolvedRequest);
    const el = fixture.nativeElement as HTMLElement;
    const buttons = el.querySelectorAll('button');
    expect(buttons.length).toBe(0);
    expect(el.textContent).toContain('Approved');
  });

  it('shows denied state correctly', () => {
    const deniedRequest: ToolRequest = {
      ...pendingRequest,
      status: 'denied',
    };
    const fixture = createComponent(deniedRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Denied');
    expect(el.querySelector('.btn-approve')).toBeFalsy();
    expect(el.querySelector('.btn-deny')).toBeFalsy();
  });

  it('shows expired state correctly', () => {
    const expiredRequest: ToolRequest = {
      ...pendingRequest,
      status: 'expired',
    };
    const fixture = createComponent(expiredRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Expired');
  });

  it('shows error detail when present', () => {
    const errorRequest: ToolRequest = {
      ...pendingRequest,
      status: 'failed',
      error: 'Something went wrong',
    };
    const fixture = createComponent(errorRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Something went wrong');
  });

  it('shows arguments in JSON format', () => {
    const fixture = createComponent(pendingRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('email');
    expect(el.textContent).toContain('new@example.com');
  });
});
