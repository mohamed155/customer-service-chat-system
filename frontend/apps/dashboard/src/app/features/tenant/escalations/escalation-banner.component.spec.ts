import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Escalation, RoutingReason } from '../../../core/api/tenant-api.models';
import { EscalationBannerComponent } from './escalation-banner.component';

describe('EscalationBannerComponent', () => {
  const baseEscalation: Escalation = {
    id: 'e-1',
    conversationId: 'c-1',
    reason: 'customer_requested',
    requiredSkills: [],
    status: 'queued',
    routing: null,
    escalatedAt: '2026-07-14T10:00:00Z',
    closedAt: null,
  };

  function createFixture() {
    TestBed.configureTestingModule({
      imports: [EscalationBannerComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(EscalationBannerComponent);
    return fixture;
  }

  it('renders nothing when escalation is null', async () => {
    TestBed.resetTestingModule();
    const fixture = createFixture();
    fixture.componentRef.setInput('escalation', null);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.banner')).toBeFalsy();
  });

  it('renders escalated label with reason and queued state', async () => {
    TestBed.resetTestingModule();
    const fixture = createFixture();
    fixture.componentRef.setInput('escalation', baseEscalation);
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Escalated');
    expect(text).toContain('Waiting in queue');
  });

  it('shows assigned state correctly', async () => {
    TestBed.resetTestingModule();
    const fixture = createFixture();
    fixture.componentRef.setInput('escalation', {
      ...baseEscalation,
      status: 'assigned',
      routing: {
        reason: 'skill_match' as RoutingReason,
        matchedSkills: ['billing'],
        assignedMembershipId: 'm-1',
        assignedAt: '2026-07-14T10:05:00Z',
      },
    });
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Assigned to agent');
    expect(fixture.nativeElement.textContent).toContain('Skill match');
  });

  it('maps all five routing reasons to plain language', async () => {
    const expectations: Array<{ reason: RoutingReason; expected: string }> = [
      { reason: 'skill_match', expected: 'Skill match' },
      { reason: 'load_fallback', expected: 'Load fallback' },
      { reason: 'manual_claim', expected: 'Manual claim' },
      { reason: 'queue_auto', expected: 'Queue auto' },
      { reason: 'manual_reassignment', expected: 'Manual reassignment' },
    ];
    TestBed.resetTestingModule();
    const fixture = createFixture();
    for (const { reason, expected } of expectations) {
      fixture.componentRef.setInput('escalation', {
        ...baseEscalation,
        status: 'assigned',
        routing: {
          reason,
          matchedSkills: [],
          assignedMembershipId: 'm-1',
          assignedAt: '2026-07-14T10:05:00Z',
        },
      });
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain(expected);
    }
  });

  it('falls back to raw reason string when label is unknown', async () => {
    TestBed.resetTestingModule();
    const fixture = createFixture();
    fixture.componentRef.setInput('escalation', {
      ...baseEscalation,
      status: 'assigned',
      routing: {
        reason: 'custom_reason' as RoutingReason,
        matchedSkills: [],
        assignedMembershipId: 'm-1',
        assignedAt: '2026-07-14T10:05:00Z',
      },
    });
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('custom_reason');
  });
});
