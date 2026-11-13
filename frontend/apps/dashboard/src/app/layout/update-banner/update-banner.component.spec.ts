import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { AppUpdateService } from '../../core/version/app-update.service';
import { PageReloader } from '../../core/version/page-reloader';
import { UpdateBannerComponent } from './update-banner.component';

describe('UpdateBannerComponent', () => {
  let fixture: ComponentFixture<UpdateBannerComponent>;
  let update: AppUpdateService;
  let reloader: { reload: ReturnType<typeof vi.fn> };

  const banner = (): HTMLElement | null => fixture.nativeElement.querySelector('.update-banner');
  const button = (label: string): HTMLButtonElement =>
    [...fixture.nativeElement.querySelectorAll('button')].find(
      (b): b is HTMLButtonElement => (b as HTMLButtonElement).textContent?.trim() === label,
    ) as HTMLButtonElement;

  beforeEach(async () => {
    reloader = { reload: vi.fn() };
    await TestBed.configureTestingModule({
      imports: [UpdateBannerComponent],
      providers: [provideZonelessChangeDetection(), { provide: PageReloader, useValue: reloader }],
    }).compileComponents();
    update = TestBed.inject(AppUpdateService);
    fixture = TestBed.createComponent(UpdateBannerComponent);
    await fixture.whenStable();
  });

  it('shows nothing until an update is known', () => {
    expect(banner()).toBeNull();
  });

  it('offers a reload when an update is known', async () => {
    update.markUpdateAvailable();
    await fixture.whenStable();

    expect(banner()?.textContent).toContain('A new version is available');
    button('Reload').click();
    expect(reloader.reload).toHaveBeenCalledTimes(1);
  });

  it('goes away on dismiss and comes back when it is marked again', async () => {
    update.markUpdateAvailable();
    await fixture.whenStable();

    button('Dismiss').click();
    await fixture.whenStable();
    expect(banner()).toBeNull();

    update.markUpdateAvailable();
    await fixture.whenStable();
    expect(banner()).not.toBeNull();
  });
});
