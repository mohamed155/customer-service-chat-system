import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { TenantInvitation } from '../../../core/api/tenant-api.models';
import { InvitationTableComponent } from './invitation-table.component';

describe('InvitationTableComponent', () => {
  it.each([
    ['unconfigured', 'Email not configured'],
    ['queued', 'Email queued'],
    ['sent', 'Email sent'],
    ['failed', 'Email failed'],
  ] as const)('shows the persisted %s delivery result', async (emailDeliveryStatus, label) => {
    await TestBed.configureTestingModule({
      imports: [InvitationTableComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();
    const fixture = TestBed.createComponent(InvitationTableComponent);
    const invitation: TenantInvitation = {
      id: 'invitation-1',
      email: 'invitee@example.com',
      role: 'agent',
      status: 'pending',
      invitedByName: 'Owner',
      createdAt: '2026-07-13T00:00:00Z',
      expiresAt: '2026-07-20T00:00:00Z',
      emailDeliveryStatus,
    };
    fixture.componentRef.setInput('title', 'Invitations');
    fixture.componentRef.setInput('headingId', 'invitations');
    fixture.componentRef.setInput('invitations', [invitation]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain(label);
  });

  it('aligns delivery, role, status, and invited cells with their headers', async () => {
    await TestBed.configureTestingModule({
      imports: [InvitationTableComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();
    const fixture = TestBed.createComponent(InvitationTableComponent);
    fixture.componentRef.setInput('title', 'Invitations');
    fixture.componentRef.setInput('headingId', 'invitations');
    fixture.componentRef.setInput('invitations', [
      {
        id: 'i-1',
        email: 'person@example.com',
        role: 'agent',
        status: 'pending',
        invitedByName: 'Owner',
        createdAt: '2026-07-13T00:00:00Z',
        expiresAt: '2026-07-20T00:00:00Z',
        emailDeliveryStatus: 'queued',
      },
    ]);
    fixture.detectChanges();

    const headers = [...fixture.nativeElement.querySelectorAll('th')].map((cell: Element) =>
      cell.textContent?.trim(),
    );
    const cells = [...fixture.nativeElement.querySelectorAll('tbody td')].map((cell: Element) =>
      cell.textContent?.trim(),
    );
    expect(headers).toEqual(['Email', 'Role', 'Status', 'Email delivery', 'Invited']);
    expect(cells).toEqual([
      'person@example.com',
      'agent',
      'Pending',
      'Email queued',
      'Jul 13, 2026',
    ]);
  });
});
