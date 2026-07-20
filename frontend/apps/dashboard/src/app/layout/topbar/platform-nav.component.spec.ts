import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { PermissionsService } from '../../core/authz/permissions.service';
import { PlatformNavComponent } from './platform-nav.component';

describe('PlatformNavComponent', () => {
  async function setup(hasPermission: boolean) {
    const permissionsMock = { has: vi.fn().mockReturnValue(hasPermission) };
    const routerMock = { navigate: vi.fn() };

    TestBed.configureTestingModule({
      imports: [PlatformNavComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: PermissionsService, useValue: permissionsMock },
        { provide: Router, useValue: routerMock },
      ],
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(PlatformNavComponent);
    fixture.detectChanges();
    return { fixture, permissionsMock, routerMock };
  }

  it('is hidden for tenant users (no platform.admin)', async () => {
    const { fixture } = await setup(false);
    const element = fixture.nativeElement as HTMLElement;
    expect(element.querySelector('.nav')).toBeNull();
    expect(element.textContent).not.toContain('Platform');
  });

  it('is hidden for platform roles without platform.admin', async () => {
    const { fixture } = await setup(false);
    const element = fixture.nativeElement as HTMLElement;
    expect(element.querySelector('.nav')).toBeNull();
  });

  it('is visible with "Platform overview" entry for Super Admin', async () => {
    const { fixture } = await setup(true);
    const element = fixture.nativeElement as HTMLElement;
    expect(element.querySelector('.trigger')).toBeTruthy();
    expect(element.textContent).toContain('Platform');
  });

  it('has correct ARIA menu semantics', async () => {
    const { fixture } = await setup(true);
    const element = fixture.nativeElement as HTMLElement;
    const trigger = element.querySelector('.trigger') as HTMLElement;
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    trigger.click();
    fixture.detectChanges();

    const dropdown = element.querySelector('.dropdown') as HTMLElement;
    expect(dropdown.getAttribute('role')).toBe('menu');

    const option = element.querySelector('.option') as HTMLElement;
    expect(option.getAttribute('role')).toBe('menuitem');
  });

  it('opens dropdown when trigger is clicked', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    const dropdown = (fixture.nativeElement as HTMLElement).querySelector('.dropdown');
    expect(dropdown).toBeTruthy();
  });

  it('closes dropdown when trigger is clicked again', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    trigger.click();
    fixture.detectChanges();
    const dropdown = (fixture.nativeElement as HTMLElement).querySelector('.dropdown');
    expect(dropdown).toBeNull();
  });

  it('navigates and closes when an entry is clicked', async () => {
    const { fixture, routerMock } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();

    const options = (fixture.nativeElement as HTMLElement).querySelectorAll('.option');
    // The first destination is "Tenants" (US2 added platform.tenants.list);
    // the second is the original "Platform overview" (Super Admin only).
    expect(options[0].textContent).toContain('Tenants');
    expect(options[1].textContent).toContain('Audit Logs');
    expect(options[2].textContent).toContain('Platform overview');

    (options[2] as HTMLElement).click();
    fixture.detectChanges();

    expect(routerMock.navigate).toHaveBeenCalledWith(['/platform']);
    const dropdown = (fixture.nativeElement as HTMLElement).querySelector('.dropdown');
    expect(dropdown).toBeNull();
  });

  it('navigates to the Tenants entry when clicked', async () => {
    const { fixture, routerMock } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();

    const options = (fixture.nativeElement as HTMLElement).querySelectorAll('.option');
    (options[0] as HTMLElement).click();
    fixture.detectChanges();

    expect(routerMock.navigate).toHaveBeenCalledWith(['/platform/tenants']);
  });

  it('closes on outside click', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    document.body.click();
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });

  it('closes on escape', async () => {
    const { fixture } = await setup(true);
    const trigger = (fixture.nativeElement as HTMLElement).querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeTruthy();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
  });
});
