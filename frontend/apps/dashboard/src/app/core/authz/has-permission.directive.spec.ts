import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { PermissionsService } from './permissions.service';
import { HasPermissionDirective } from './has-permission.directive';

describe('HasPermissionDirective', () => {
  let permissionSignal: ReturnType<typeof signal<boolean>>;

  @Component({
    imports: [HasPermissionDirective],
    template: ` <div *appHasPermission="'overview.view'" class="guarded">Secret</div> `,
  })
  class TestComponent {}

  const configure = (hasResult: boolean) => {
    permissionSignal = signal(hasResult);
    const permissions = { has: vi.fn(() => permissionSignal()) };
    TestBed.configureTestingModule({
      providers: [{ provide: PermissionsService, useValue: permissions }],
    });
  };

  it('renders content when permission is held', async () => {
    configure(true);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TestComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Secret');
  });

  it('hides content when permission is not held', async () => {
    configure(false);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TestComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).not.toContain('Secret');
  });

  it('reactively updates when permission changes', async () => {
    configure(false);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(TestComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain('Secret');

    permissionSignal.set(true);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Secret');

    permissionSignal.set(false);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain('Secret');
  });
});
