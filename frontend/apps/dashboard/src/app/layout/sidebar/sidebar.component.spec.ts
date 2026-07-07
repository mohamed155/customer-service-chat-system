import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { SidebarComponent } from './sidebar.component';

describe('SidebarComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [SidebarComponent],
      providers: [provideRouter([]), provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('renders grouped Helix tenant navigation with a conversations badge', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-sidebar-nav-group').length).toBe(4);
    expect(element.querySelectorAll('app-sidebar-nav-item').length).toBe(8);
    expect(element.textContent).toContain('Workspace');
    expect(element.textContent).toContain('AI');
    expect(element.textContent).toContain('Insights');
    expect(element.textContent).toContain('Settings');
    expect(element.textContent).toContain('Conversations');
    expect(element.textContent).toContain('6');
  });

  it('hides labels and adds item aria-labels when collapsed', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.componentRef.setInput('collapsed', true);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).not.toContain('Workspace');
    expect(element.querySelector('a[aria-label="Overview"]')).toBeTruthy();
    expect(element.querySelector('a[aria-label="Conversations"]')).toBeTruthy();
  });
});
