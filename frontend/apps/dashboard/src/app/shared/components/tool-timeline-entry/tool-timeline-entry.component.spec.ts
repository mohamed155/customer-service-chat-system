import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ToolRequest } from '../../../core/api/tenant-api.models';
import { ToolTimelineEntryComponent } from './tool-timeline-entry.component';

describe('ToolTimelineEntryComponent', () => {
  const succeededRequest: ToolRequest = {
    id: 'tr-1',
    toolName: 'lookup_customer',
    toolSource: 'builtin',
    arguments: {},
    status: 'succeeded',
    approvalRequired: false,
    chainIndex: 0,
    createdAt: '2026-07-18T10:00:00Z',
    durationMs: 412,
    result: { displayName: 'Maya Chen', email: 'maya@example.com' },
  };

  function createComponent(request: ToolRequest) {
    TestBed.configureTestingModule({
      imports: [ToolTimelineEntryComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ToolTimelineEntryComponent);
    fixture.componentRef.setInput('request', request);
    fixture.detectChanges();
    return fixture;
  }

  it('renders tool name and status badge', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('lookup_customer');
    expect(el.textContent).toContain('succeeded');
  });

  it('renders chain index badge', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('#0');
  });

  it('renders duration when present', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('412ms');
  });

  it('shows approval-required styling when approvalRequired is true', () => {
    const approvalRequest: ToolRequest = {
      ...succeededRequest,
      id: 'tr-2',
      toolName: 'update_customer_contact',
      approvalRequired: true,
      status: 'awaiting_approval',
    };
    const fixture = createComponent(approvalRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.approval-required')).toBeTruthy();
    expect(el.textContent).toContain('awaiting_approval');
  });

  it('shows error detail when error is present', () => {
    const failedRequest: ToolRequest = {
      ...succeededRequest,
      id: 'tr-3',
      status: 'failed',
      error: 'Customer not found',
      durationMs: 210,
    };
    const fixture = createComponent(failedRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Customer not found');
    expect(el.querySelector('.status-error')).toBeTruthy();
  });

  it('renders without duration when durationMs is null', () => {
    const noDurationRequest: ToolRequest = {
      ...succeededRequest,
      durationMs: undefined,
    };
    const fixture = createComponent(noDurationRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).not.toContain('ms');
    expect(el.querySelector('.duration')).toBeFalsy();
  });

  it('shows decider name when present', () => {
    const decidedRequest: ToolRequest = {
      ...succeededRequest,
      id: 'tr-4',
      status: 'denied',
      decidedByDisplayName: 'Dana A.',
    };
    const fixture = createComponent(decidedRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('by Dana A.');
  });

  it('applies error styling for failed statuses', () => {
    const failedRequest: ToolRequest = {
      ...succeededRequest,
      id: 'tr-5',
      status: 'timed_out',
      error: 'timeout',
    };
    const fixture = createComponent(failedRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.entry-error')).toBeTruthy();
  });

  it('applies success styling for success status', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.entry-success')).toBeTruthy();
  });

  it('embeds tool-result-viewer', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('app-tool-result-viewer')).toBeTruthy();
  });

  it('shows arguments toggle button when arguments are present', () => {
    const fixture = createComponent(succeededRequest);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Show arguments');
  });

  it('expands to show arguments JSON on click', () => {
    const requestWithArgs: ToolRequest = {
      ...succeededRequest,
      id: 'tr-6',
      toolName: 'update_customer',
      arguments: { field: 'email', value: 'new@example.com' },
    };
    const fixture = createComponent(requestWithArgs);
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector('.args-toggle') as HTMLButtonElement;
    expect(button).toBeTruthy();
    button.click();
    fixture.detectChanges();
    expect(el.textContent).toContain('"field"');
    expect(el.textContent).toContain('"email"');
    expect(el.textContent).toContain('"new@example.com"');
  });
});
