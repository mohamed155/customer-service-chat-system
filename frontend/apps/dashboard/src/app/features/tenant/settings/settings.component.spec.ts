import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { Permission } from '../../../core/authz/permissions';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { appUiActions } from '../../../core/state/app-ui.feature';
import { SettingsComponent } from './settings.component';

describe('SettingsComponent', () => {
  function mount(permissions: Permission[] = [], themeMode: string = 'system') {
    TestBed.configureTestingModule({
      imports: [SettingsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        provideMockStore({
          initialState: {
            appUi: { themeMode, sidebarCollapsed: false },
            tenantContext: {
              activeTenant: { id: 't1', name: 'Acme Support', slug: 'acme', status: 'active' },
              status: 'idle',
            },
          },
        }),
        {
          provide: PermissionsService,
          useValue: { has: (p: Permission) => permissions.includes(p) },
        },
      ],
    });
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();
    return fixture;
  }

  it('shows the real workspace, not a fixture profile', async () => {
    const fixture = mount();
    await fixture.whenStable();
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Acme Support');
    expect(text).toContain('acme');
    expect(text).not.toContain('Helix Support Ops');
    expect(text).not.toContain('helix.example.com');
  });

  it('has no fake controls: no tabs, no inputs and no toggles', () => {
    const element = mount(['members.view', 'billing.view', 'tools.view'])
      .nativeElement as HTMLElement;

    expect(element.querySelector('[role="tablist"]')).toBeFalsy();
    expect(element.querySelector('input')).toBeFalsy();
    expect(element.textContent).not.toMatch(/API key|Two-factor|Visual switch|fixture/i);
  });

  it('dispatches theme changes and marks the current theme', () => {
    const fixture = mount([], 'dark');
    const store = TestBed.inject(MockStore);
    const dispatch = vi.spyOn(store, 'dispatch');
    const buttons = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
        '.segmented button',
      ),
    );

    expect(buttons.find((b) => b.getAttribute('aria-pressed') === 'true')?.textContent).toContain(
      'Dark',
    );
    buttons.find((b) => b.textContent?.includes('Light'))?.click();

    expect(dispatch).toHaveBeenCalledWith(appUiActions.themeModeChanged({ themeMode: 'light' }));
  });

  it('links only to the pages the person may open', () => {
    const fixture = mount(['members.view', 'tools.view']);
    fixture.detectChanges();

    const hrefs = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('li a')).map(
      (a) => a.getAttribute('href'),
    );
    expect(hrefs).toEqual(['/tenant/team', '/tenant/settings/tools']);
  });
});
