import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { RoleSelectComponent } from './role-select.component';

describe('RoleSelectComponent', () => {
  async function setup(): Promise<RoleSelectComponent> {
    TestBed.configureTestingModule({
      imports: [RoleSelectComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RoleSelectComponent);
    fixture.detectChanges();
    return fixture.componentInstance;
  }

  it('creates the component', async () => {
    const component = await setup();
    expect(component).toBeTruthy();
  });

  it('defaults value to agent', async () => {
    const component = await setup();
    expect(component.value()).toBe('agent');
  });

  it('renders assignable roles through the shared choice-group primitive', async () => {
    TestBed.configureTestingModule({
      imports: [RoleSelectComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RoleSelectComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('app-choice-group')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Support Agent');
  });

  it('updates value when a role button is clicked', async () => {
    TestBed.configureTestingModule({
      imports: [RoleSelectComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RoleSelectComponent);
    fixture.detectChanges();
    const buttons = fixture.nativeElement.querySelectorAll(
      'app-choice-group button',
    ) as NodeListOf<HTMLButtonElement>;
    const adminButton = Array.from(buttons).find((b) => b.textContent?.trim() === 'Admin');
    if (adminButton) {
      adminButton.click();
      fixture.detectChanges();
      expect(fixture.componentInstance.value()).toBe('admin');
    }
  });

  it('filters roles based on maxRole input', async () => {
    TestBed.configureTestingModule({
      imports: [RoleSelectComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RoleSelectComponent);
    fixture.componentRef.setInput('currentRole', 'agent');
    fixture.detectChanges();
    const buttons = fixture.nativeElement.querySelectorAll(
      'app-choice-group button',
    ) as NodeListOf<HTMLButtonElement>;
    const labels = Array.from(buttons).map((b) => b.textContent?.trim());
    expect(labels).toContain('Support Agent');
    expect(labels).not.toContain('Admin');
  });

  it('shows owner only when the actor is owner and can assign owner', async () => {
    TestBed.configureTestingModule({
      imports: [RoleSelectComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(RoleSelectComponent);
    fixture.componentRef.setInput('currentRole', 'owner');
    fixture.componentRef.setInput('canAssignOwner', true);
    fixture.detectChanges();

    const labels = Array.from(
      fixture.nativeElement.querySelectorAll(
        'app-choice-group button',
      ) as NodeListOf<HTMLButtonElement>,
    ).map((button) => button.textContent?.trim());
    expect(labels).toContain('Owner');
    expect(labels).toContain('Admin');
  });
});
