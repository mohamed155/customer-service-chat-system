import { provideZonelessChangeDetection, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Store } from '@ngrx/store';
import { provideTaiga } from '@taiga-ui/core';
import { Subject, of, throwError } from 'rxjs';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { CustomerFixture } from '../../../shared/fixtures/fixture.models';
import { RoutedPageDataService } from '../routed-page-data.service';
import { CustomersComponent } from './customers.component';

describe('CustomersComponent', () => {
  const loadCustomers = vi.fn();
  const activeTenant = signal<{ id: string } | null>({ id: 'tenant-1' });

  beforeEach(() => {
    activeTenant.set({ id: 'tenant-1' });
    loadCustomers.mockReset();
    TestBed.configureTestingModule({
      imports: [CustomersComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadCustomers } },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
      ],
    });
  });

  it('moves from pending to the shared zero-data state', async () => {
    const loader = new Subject<{ page: string; data: readonly CustomerFixture[] }>();
    loadCustomers.mockReturnValue(loader.asObservable());
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    loader.next({ page: 'customers', data: [] });
    loader.complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
    expect(fixture.nativeElement.textContent).toContain('No customers yet');
  });

  it('shows the shared no-results state and resets the search', async () => {
    loadCustomers.mockReturnValue(of({ page: 'customers', data: CUSTOMER_FIXTURES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-data-table')).toBeTruthy();
    });

    const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
    input.value = 'no matching customer';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('No customers match');
    const reset = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((button) => button.textContent?.trim() === 'Clear search')!;
    reset.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-data-table')).toBeTruthy();
  });

  it('transitions from tenant A content to tenant B content', async () => {
    const aData = CUSTOMER_FIXTURES.slice(0, 2);
    const bData = CUSTOMER_FIXTURES.slice(2, 4);

    loadCustomers.mockReturnValue(of({ page: 'customers', data: aData }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadCustomers.mockReturnValue(of({ page: 'customers', data: bData }));
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('moves from pending to error and retries', async () => {
    loadCustomers.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadCustomers.mockReturnValue(of({ page: 'customers', data: CUSTOMER_FIXTURES }));
    const retryBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Try again')!;
    retryBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });

  it('prevents stale tenant A data from reappearing after tenant B resolves first', async () => {
    const aData = CUSTOMER_FIXTURES.slice(0, 2);
    const bData = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<{
      page: string;
      data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
    }>[] = [];

    loadCustomers.mockImplementation(() => {
      const s = new Subject<{
        page: string;
        data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
      }>();
      subjects.push(s);
      return s.asObservable();
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    await vi.waitFor(() => expect(loadCustomers).toHaveBeenCalledTimes(1));

    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => expect(loadCustomers).toHaveBeenCalledTimes(2));

    subjects[1].next({ page: 'customers', data: bData });
    subjects[1].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });

    subjects[0].next({ page: 'customers', data: aData });
    subjects[0].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });

  it('removes tenant A content while tenant B is pending', async () => {
    const aData = CUSTOMER_FIXTURES.slice(0, 2);
    const bData = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<{
      page: string;
      data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
    }>[] = [];

    loadCustomers.mockReturnValue(of({ page: 'customers', data: aData }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadCustomers.mockImplementation(() => {
      const s = new Subject<{
        page: string;
        data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
      }>();
      subjects.push(s);
      return s.asObservable();
    });
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    expect(fixture.nativeElement.querySelector('app-loading-state')).toBeTruthy();
    expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');

    subjects[0].next({ page: 'customers', data: bData });
    subjects[0].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });
  });

  it('handles tenant B resolving to empty', async () => {
    const aData = CUSTOMER_FIXTURES.slice(0, 2);

    loadCustomers.mockReturnValue(of({ page: 'customers', data: aData }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Maya Chen');
    });

    loadCustomers.mockReturnValue(of({ page: 'customers', data: [] }));
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
    expect(fixture.nativeElement.textContent).toContain('No customers yet');
  });

  it('ignores rejected tenant A after tenant B loads successfully', async () => {
    const bData = CUSTOMER_FIXTURES.slice(2, 4);
    const subjects: Subject<{
      page: string;
      data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
    }>[] = [];

    loadCustomers.mockImplementation(() => {
      const s = new Subject<{
        page: string;
        data: readonly import('../../../shared/fixtures/fixture.models').CustomerFixture[];
      }>();
      subjects.push(s);
      return s.asObservable();
    });

    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(CustomersComponent);
    fixture.detectChanges();
    await vi.waitFor(() => expect(loadCustomers).toHaveBeenCalledTimes(1));

    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();
    await vi.waitFor(() => expect(loadCustomers).toHaveBeenCalledTimes(2));

    subjects[1].next({ page: 'customers', data: bData });
    subjects[1].complete();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
    });

    subjects[0].error(new Error('A failed'));
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Ava Patel');
      expect(fixture.nativeElement.textContent).not.toContain('Maya Chen');
    });
  });
});
