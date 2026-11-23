import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import {
  PlatformAccessSession,
  TenantContextService,
} from '../../core/tenant/tenant-context.service';
import { PlatformAccessBannerComponent } from './platform-access-banner.component';

const session = (minutesLeft: number): PlatformAccessSession => ({
  id: 'access-1',
  tenantId: 't-1',
  tenantName: 'Acme Support',
  reason: 'Reviewing a support request',
  startedAt: new Date(Date.now() - 60_000).toISOString(),
  expiresAt: new Date(Date.now() + minutesLeft * 60_000).toISOString(),
});

describe('PlatformAccessBannerComponent', () => {
  let fixture: ComponentFixture<PlatformAccessBannerComponent>;
  const accessSession = signal<PlatformAccessSession | null>(null);
  const endAccess = vi.fn();
  const expireAccess = vi.fn();
  const status = () => fixture.nativeElement.querySelector('[role="status"]') as HTMLElement | null;

  beforeEach(() => {
    accessSession.set(null);
    endAccess.mockReset().mockResolvedValue(undefined);
    expireAccess.mockReset();
    TestBed.configureTestingModule({
      imports: [PlatformAccessBannerComponent],
      providers: [
        provideRouter([]),
        {
          provide: TenantContextService,
          useValue: { accessSession, endAccess, expireAccess, status: () => 'idle' },
        },
      ],
    });
    fixture = TestBed.createComponent(PlatformAccessBannerComponent);
    fixture.detectChanges();
  });

  it('is hidden without an access session', () => {
    expect(status()).toBeNull();
  });

  it('shows the tenant and time left and returns to the directory when access ends', async () => {
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    accessSession.set(session(25));
    fixture.detectChanges();

    expect(status()?.textContent).toContain('Acme Support');
    expect(status()?.textContent).toContain('25 minutes left');
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    await fixture.whenStable();

    expect(endAccess).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledExactlyOnceWith(['/platform', 'tenants']);
  });

  it('replaces a countdown with a link to start new access when the session expires', () => {
    accessSession.set(session(-1));
    fixture.detectChanges();

    expect(expireAccess).toHaveBeenCalledExactlyOnceWith('access-1');
    expect(status()?.textContent).toContain('Access to Acme Support has expired');
    expect(status()?.textContent).not.toContain('minutes left');
    const link = fixture.nativeElement.querySelector('a') as HTMLAnchorElement;
    expect(link.textContent?.trim()).toBe('Start new access');
    expect(link.getAttribute('href')).toBe('/platform/tenants/t-1');
  });
});
