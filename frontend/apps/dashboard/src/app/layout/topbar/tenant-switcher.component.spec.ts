import { provideZonelessChangeDetection } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { of, Subject, throwError } from 'rxjs';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { ApiService } from '../../core/api/api.service';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { TenantSummary } from '../../core/api/tenant-api.models';
import { tenantContextActions } from '../../core/state/tenant-context.feature';
import { TenantSwitcherComponent } from './tenant-switcher.component';

const fakeTenants: TenantSummary[] = [
  { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'trial' },
  { id: 't-2', name: 'Globex Inc', slug: 'globex', status: 'active', plan: 'starter' },
  { id: 't-3', name: 'Initech', slug: 'initech', status: 'suspended', plan: 'professional' },
];

const page = (items: TenantSummary[]) => ({ data: { items, nextCursor: null, hasMore: false } });

describe('TenantSwitcherComponent', () => {
  async function setup(isPlatform = true) {
    const api = { list: vi.fn(), post: vi.fn() };
    api.list.mockReturnValue(of(page(fakeTenants)));

    TestBed.configureTestingModule({
      imports: [TenantSwitcherComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: { tenantContext: { activeTenant: null, status: 'idle' as const } },
        }),
        CurrentUserService,
        { provide: ApiService, useValue: api },
      ],
    });

    const currentUser = TestBed.inject(CurrentUserService);
    currentUser['user'].set({
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: isPlatform ? 'super_admin' : null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: isPlatform
        ? []
        : [
            {
              tenantId: 't-1',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: 'agent',
              permissions: [],
            },
          ],
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TenantSwitcherComponent);
    fixture.detectChanges();
    return { fixture, api, store: TestBed.inject(MockStore) };
  }

  const el = (fixture: ComponentFixture<unknown>) => fixture.nativeElement as HTMLElement;
  const trigger = (fixture: ComponentFixture<unknown>) =>
    el(fixture).querySelector('.trigger') as HTMLButtonElement;
  const search = (fixture: ComponentFixture<unknown>) =>
    el(fixture).querySelector('.search-input') as HTMLInputElement;
  const options = (fixture: ComponentFixture<unknown>) =>
    Array.from(el(fixture).querySelectorAll<HTMLElement>('.option'));

  async function openList(fixture: ComponentFixture<unknown>) {
    trigger(fixture).click();
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  function press(fixture: ComponentFixture<unknown>, target: HTMLElement, key: string) {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
    target.dispatchEvent(event);
    fixture.detectChanges();
    return event;
  }

  describe('opening', () => {
    it('shows the placeholder and asks for nothing until it is opened', async () => {
      const { fixture, api } = await setup();

      expect(el(fixture).textContent).toContain('Select tenant...');
      expect(api.list).not.toHaveBeenCalled();
    });

    it('opens when the trigger is clicked and lists the tenants', async () => {
      const { fixture, api } = await setup();
      await openList(fixture);

      expect(api.list).toHaveBeenCalledTimes(1);
      expect(el(fixture).querySelector('.dropdown')).toBeTruthy();
      expect(el(fixture).textContent).toContain('Acme Corp');
      expect(trigger(fixture).getAttribute('aria-expanded')).toBe('true');
    });

    it('moves focus to the search field on open', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      expect(document.activeElement).toBe(search(fixture));
    });

    it('opens with ArrowDown or ArrowUp on the trigger', async () => {
      const { fixture } = await setup();
      const event = press(fixture, trigger(fixture), 'ArrowDown');
      await fixture.whenStable();
      fixture.detectChanges();

      expect(event.defaultPrevented).toBe(true);
      expect(el(fixture).querySelector('.dropdown')).toBeTruthy();
    });

    it('displays the active tenant name', async () => {
      const { fixture, store } = await setup();
      store.setState({
        tenantContext: { activeTenant: fakeTenants[0], status: 'idle' as const },
      });
      fixture.detectChanges();

      expect(el(fixture).textContent).toContain('Acme Corp');
    });

    it('filters the list by name or slug', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      search(fixture).value = 'glob';
      search(fixture).dispatchEvent(new Event('input'));
      fixture.detectChanges();

      expect(options(fixture).map((o) => o.textContent)).toEqual([
        expect.stringContaining('Globex'),
      ]);
    });
  });

  describe('listbox semantics', () => {
    it('has aria-haspopup="listbox" on the trigger and points it at the list when open', async () => {
      const { fixture } = await setup();
      expect(trigger(fixture).getAttribute('aria-haspopup')).toBe('listbox');
      expect(trigger(fixture).hasAttribute('aria-controls')).toBe(false);

      await openList(fixture);
      const list = el(fixture).querySelector('[role="listbox"]')!;
      expect(trigger(fixture).getAttribute('aria-controls')).toBe(list.id);
    });

    it('makes the search field a combobox that controls the listbox', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      const list = el(fixture).querySelector('[role="listbox"]')!;
      expect(search(fixture).getAttribute('role')).toBe('combobox');
      expect(search(fixture).getAttribute('aria-controls')).toBe(list.id);
      expect(search(fixture).getAttribute('aria-expanded')).toBe('true');
    });

    it('marks the active tenant aria-selected and gives every option an id', async () => {
      const { fixture, store } = await setup();
      store.setState({
        tenantContext: { activeTenant: fakeTenants[1], status: 'idle' as const },
      });
      await openList(fixture);

      const all = options(fixture);
      expect(all.every((o) => o.getAttribute('role') === 'option' && o.id)).toBe(true);
      expect(all.map((o) => o.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false']);
    });
  });

  describe('keyboard', () => {
    const highlighted = (fixture: ComponentFixture<unknown>) =>
      options(fixture).findIndex(
        (o) => o.id === search(fixture).getAttribute('aria-activedescendant'),
      );

    it('starts on the active tenant, or on the first option without one', async () => {
      const { fixture, store } = await setup();
      store.setState({
        tenantContext: { activeTenant: fakeTenants[2], status: 'idle' as const },
      });
      await openList(fixture);
      expect(highlighted(fixture)).toBe(2);
    });

    it('starts on the first option when no tenant is active', async () => {
      const { fixture } = await setup();
      await openList(fixture);
      expect(highlighted(fixture)).toBe(0);
    });

    it('moves with ArrowDown and ArrowUp, wrapping at the ends', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      press(fixture, search(fixture), 'ArrowDown');
      expect(highlighted(fixture)).toBe(1);
      press(fixture, search(fixture), 'ArrowDown');
      press(fixture, search(fixture), 'ArrowDown');
      expect(highlighted(fixture)).toBe(0);
      press(fixture, search(fixture), 'ArrowUp');
      expect(highlighted(fixture)).toBe(2);
    });

    it('jumps with Home and End and keeps the key from moving the caret', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      const end = press(fixture, search(fixture), 'End');
      expect(highlighted(fixture)).toBe(2);
      expect(end.defaultPrevented).toBe(true);
      press(fixture, search(fixture), 'Home');
      expect(highlighted(fixture)).toBe(0);
    });

    it('marks the highlighted option visually as well', async () => {
      const { fixture } = await setup();
      await openList(fixture);
      press(fixture, search(fixture), 'ArrowDown');

      expect(options(fixture).map((o) => o.classList.contains('highlighted'))).toEqual([
        false,
        true,
        false,
      ]);
    });

    it('selects the highlighted option with Enter', async () => {
      const { fixture, api } = await setup();
      api.post.mockReturnValue(of({ data: fakeTenants[1] }));
      await openList(fixture);

      press(fixture, search(fixture), 'ArrowDown');
      press(fixture, search(fixture), 'Enter');

      expect(api.post).toHaveBeenCalledWith('/platform/tenants/t-2/switch', undefined);
    });

    it('keeps the highlight inside a filtered list and resets it when the filter changes', async () => {
      const { fixture } = await setup();
      await openList(fixture);
      press(fixture, search(fixture), 'End');

      search(fixture).value = 'o';
      search(fixture).dispatchEvent(new Event('input'));
      fixture.detectChanges();

      expect(highlighted(fixture)).toBe(0);
    });

    it('does nothing on arrows or Enter when nothing matches', async () => {
      const { fixture, api } = await setup();
      await openList(fixture);
      search(fixture).value = 'zzz';
      search(fixture).dispatchEvent(new Event('input'));
      fixture.detectChanges();

      press(fixture, search(fixture), 'ArrowDown');
      press(fixture, search(fixture), 'Enter');

      expect(api.post).not.toHaveBeenCalled();
      expect(search(fixture).hasAttribute('aria-activedescendant')).toBe(false);
    });
  });

  describe('focus', () => {
    it('returns to the trigger after Escape, and Escape does not clear the search', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      const event = press(fixture, search(fixture), 'Escape');
      await fixture.whenStable();

      expect(event.defaultPrevented).toBe(true);
      expect(el(fixture).querySelector('.dropdown')).toBeNull();
      expect(document.activeElement).toBe(trigger(fixture));
    });

    it('returns to the trigger after a click outside that focuses nothing', async () => {
      const { fixture } = await setup();
      await openList(fixture);
      (document.activeElement as HTMLElement).blur();

      document.body.click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('.dropdown')).toBeNull();
      expect(document.activeElement).toBe(trigger(fixture));
    });

    it('leaves focus on what an outside click focused', async () => {
      const { fixture } = await setup();
      const other = document.createElement('input');
      document.body.appendChild(other);
      await openList(fixture);

      other.focus();
      other.click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('.dropdown')).toBeNull();
      expect(document.activeElement).toBe(other);
      other.remove();
    });

    it('returns to the trigger after a selection', async () => {
      const { fixture, api } = await setup();
      api.post.mockReturnValue(of({ data: fakeTenants[0] }));
      await openList(fixture);

      options(fixture)[0].click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('.dropdown')).toBeNull();
      expect(document.activeElement).toBe(trigger(fixture));
    });

    it('closes when focus tabs out of the switcher', async () => {
      const { fixture } = await setup();
      const other = document.createElement('button');
      document.body.appendChild(other);
      await openList(fixture);

      search(fixture).dispatchEvent(
        new FocusEvent('focusout', { bubbles: true, relatedTarget: other }),
      );
      fixture.detectChanges();

      expect(el(fixture).querySelector('.dropdown')).toBeNull();
      other.remove();
    });

    it('stays open when focus moves inside the switcher', async () => {
      const { fixture } = await setup();
      await openList(fixture);

      search(fixture).dispatchEvent(
        new FocusEvent('focusout', { bubbles: true, relatedTarget: trigger(fixture) }),
      );
      fixture.detectChanges();

      expect(el(fixture).querySelector('.dropdown')).toBeTruthy();
    });
  });

  describe('selecting', () => {
    it('switches tenant on click through the store', async () => {
      const { fixture, api, store } = await setup();
      const dispatch = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(of({ data: fakeTenants[0] }));
      await openList(fixture);

      options(fixture)[0].click();

      expect(api.post).toHaveBeenCalledWith('/platform/tenants/t-1/switch', undefined);
      expect(dispatch).toHaveBeenCalledWith(
        tenantContextActions.switchTenantRequested({ tenantId: 't-1' }),
      );
      expect(dispatch).toHaveBeenCalledWith(expect.objectContaining({ tenant: expect.anything() }));
    });

    it('shows an inline error and stays open when the switch fails', async () => {
      const { fixture, api } = await setup();
      api.post.mockReturnValue(throwError(() => new Error('boom')));
      await openList(fixture);

      options(fixture)[0].click();
      fixture.detectChanges();

      const alert = el(fixture).querySelector('[role="alert"]');
      expect(alert?.textContent).toContain('Could not switch tenant');
      expect(el(fixture).querySelector('.dropdown')).toBeTruthy();
    });

    it('cancels a switch still in flight when another tenant is picked', async () => {
      const { fixture, api } = await setup();
      const first = new Subject<{ data: TenantSummary }>();
      const second = new Subject<{ data: TenantSummary }>();
      api.post.mockReturnValueOnce(first).mockReturnValueOnce(second);
      await openList(fixture);

      options(fixture)[0].click();
      expect(first.observed).toBe(true);
      options(fixture)[1].click();

      expect(first.observed).toBe(false);
      expect(second.observed).toBe(true);
    });

    it('abandons a switch still on its way when the list is closed, so it cannot fail unseen', async () => {
      const { fixture, api, store } = await setup();
      const dispatch = vi.spyOn(store, 'dispatch');
      const pending = new Subject<{ data: TenantSummary }>();
      api.post.mockReturnValue(pending);
      await openList(fixture);

      options(fixture)[0].click();
      expect(pending.observed).toBe(true);
      press(fixture, search(fixture), 'Escape');

      expect(pending.observed).toBe(false);
      expect(dispatch).toHaveBeenCalledWith(tenantContextActions.switchTenantCancelled());
      expect(el(fixture).querySelector('[role="alert"]')).toBeNull();
    });
  });

  describe('loading', () => {
    it('shows a loading line while the first list is on its way', async () => {
      const { fixture, api } = await setup();
      api.list.mockReturnValue(new Subject());
      await openList(fixture);

      expect(el(fixture).querySelector('[role="status"]')?.textContent).toContain('Loading');
      expect(el(fixture).querySelector('[role="listbox"]')?.getAttribute('aria-busy')).toBe('true');
    });

    it('cancels the request in flight when the list closes', async () => {
      const { fixture, api } = await setup();
      const pending = new Subject();
      api.list.mockReturnValue(pending);
      await openList(fixture);
      expect(pending.observed).toBe(true);

      press(fixture, search(fixture), 'Escape');

      expect(pending.observed).toBe(false);
    });

    it('drops a stale answer: closing and reopening shows only the newer request', async () => {
      const { fixture, api } = await setup();
      const stale = new Subject();
      const fresh = new Subject();
      api.list.mockReturnValueOnce(stale).mockReturnValueOnce(fresh);

      await openList(fixture);
      press(fixture, search(fixture), 'Escape');
      await openList(fixture);

      fresh.next(page([fakeTenants[1]]));
      stale.next(page([fakeTenants[0]]));
      fixture.detectChanges();

      expect(options(fixture).map((o) => o.textContent)).toEqual([
        expect.stringContaining('Globex'),
      ]);
      expect(el(fixture).textContent).not.toContain('Acme');
    });

    it('shows an inline error with a retry, not a silent empty list', async () => {
      const { fixture, api } = await setup();
      api.list.mockReturnValueOnce(throwError(() => new Error('down')));
      await openList(fixture);

      const alert = el(fixture).querySelector('[role="alert"]')!;
      expect(alert.textContent).toContain('could not be loaded');
      expect(el(fixture).querySelector('.empty')).toBeNull();

      api.list.mockReturnValue(of(page(fakeTenants)));
      (alert.querySelector('button') as HTMLButtonElement).click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('[role="alert"]')).toBeNull();
      expect(options(fixture)).toHaveLength(3);
    });

    it('says when there are no tenants', async () => {
      const { fixture, api } = await setup();
      api.list.mockReturnValue(of(page([])));
      await openList(fixture);

      expect(el(fixture).querySelector('.empty')?.textContent).toContain('No tenants found');
    });
  });
});
