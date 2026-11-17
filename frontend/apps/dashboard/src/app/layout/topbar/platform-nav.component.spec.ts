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
    expect(options[0].textContent).toContain('Tenants');
    expect(options[1].textContent).toContain('Audit Logs');
    expect(options[2].textContent).toContain('Billing');
    expect(options[3].textContent).toContain('System health');
    expect(options[4].textContent).toContain('AI providers');
    expect(options[5].textContent).toContain('Platform overview');

    (options[5] as HTMLElement).click();
    fixture.detectChanges();

    expect(routerMock.navigate).toHaveBeenCalledWith(['/platform/overview-placeholder']);
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

  it('does not take focus on an Escape pressed elsewhere while its menu is closed', async () => {
    const { fixture } = await setup(true);
    const other = document.createElement('input');
    document.body.appendChild(other);
    other.focus();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();

    expect(document.activeElement).toBe(other);
    other.remove();
  });
  describe('focus', () => {
    function open(fixture: { nativeElement: unknown; detectChanges(): void }) {
      const host = fixture.nativeElement as HTMLElement;
      const trigger = host.querySelector('.trigger') as HTMLElement;
      trigger.click();
      fixture.detectChanges();
      return { host, trigger };
    }

    it('moves focus to the first menu item whenever it opens', async () => {
      const { fixture } = await setup(true);
      const { host, trigger } = open(fixture);
      await fixture.whenStable();
      const options = Array.from(host.querySelectorAll<HTMLElement>('.option'));
      expect(options.length).toBeGreaterThan(1);
      expect(document.activeElement).toBe(options[0]);

      // Closed with the trigger, then opened again: the first item takes focus again.
      trigger.click();
      fixture.detectChanges();
      expect(document.activeElement).toBe(trigger);
      trigger.click();
      fixture.detectChanges();
      await fixture.whenStable();
      expect(document.activeElement).toBe(host.querySelector('.option'));
    });

    it('puts focus back on the trigger after Escape', async () => {
      const { fixture } = await setup(true);
      const { trigger } = open(fixture);
      await fixture.whenStable();

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();
      expect(document.activeElement).toBe(trigger);
    });

    it('puts focus back on the trigger after a selection', async () => {
      const { fixture } = await setup(true);
      const { host, trigger } = open(fixture);
      await fixture.whenStable();

      host.querySelector<HTMLElement>('.option')!.click();
      fixture.detectChanges();
      expect(document.activeElement).toBe(trigger);
    });

    it('puts focus back on the trigger when a click outside focused nothing', async () => {
      const { fixture } = await setup(true);
      const { trigger } = open(fixture);
      await fixture.whenStable();
      (document.activeElement as HTMLElement).blur();

      document.body.click();
      fixture.detectChanges();
      expect(document.activeElement).toBe(trigger);
    });

    it('leaves focus where a click outside put it', async () => {
      const { fixture } = await setup(true);
      open(fixture);
      await fixture.whenStable();
      const other = document.createElement('input');
      document.body.appendChild(other);
      other.focus();

      other.click();
      fixture.detectChanges();
      expect((fixture.nativeElement as HTMLElement).querySelector('.dropdown')).toBeNull();
      expect(document.activeElement).toBe(other);
      other.remove();
    });
  });
});
