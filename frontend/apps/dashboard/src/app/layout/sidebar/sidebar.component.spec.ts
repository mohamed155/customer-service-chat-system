import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { Permission } from '../../core/authz/permissions';
import { PermissionsService } from '../../core/authz/permissions.service';
import { SidebarComponent } from './sidebar.component';

const configure = (allowedPermissions: Permission[]) => {
  const permissions = { has: vi.fn((p: Permission) => allowedPermissions.includes(p)) };
  TestBed.configureTestingModule({
    imports: [SidebarComponent],
    providers: [
      provideRouter([]),
      provideTaiga(),
      provideZonelessChangeDetection(),
      { provide: PermissionsService, useValue: permissions },
    ],
  });
};

describe('SidebarComponent', () => {
  it('renders full navigation with all permissions', async () => {
    configure([
      'overview.view',
      'conversations.view',
      'customers.view',
      'ai_agent.view',
      'knowledge_base.view',
      'integrations.view',
      'analytics.view',
      'settings.view',
    ]);
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
    configure([
      'overview.view',
      'conversations.view',
      'customers.view',
      'ai_agent.view',
      'knowledge_base.view',
      'integrations.view',
      'analytics.view',
      'settings.view',
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.componentRef.setInput('collapsed', true);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.textContent).not.toContain('Workspace');
    expect(element.querySelector('a[aria-label="Overview"]')).toBeTruthy();
    expect(element.querySelector('a[aria-label="Conversations"]')).toBeTruthy();
  });

  it('shows only Overview, Conversations, Customers, Knowledge Base for Support Agent', async () => {
    configure(['overview.view', 'conversations.view', 'customers.view', 'knowledge_base.view']);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-sidebar-nav-group').length).toBe(2);
    expect(element.querySelectorAll('app-sidebar-nav-item').length).toBe(4);
    expect(element.textContent).toContain('Workspace');
    expect(element.textContent).toContain('Conversations');
    expect(element.textContent).not.toContain('AI Agent');
    expect(element.textContent).not.toContain('Analytics');
    expect(element.textContent).not.toContain('Settings');
  });

  it('shows all view-only pages for Viewer (no Settings)', async () => {
    configure([
      'overview.view',
      'conversations.view',
      'customers.view',
      'ai_agent.view',
      'knowledge_base.view',
      'integrations.view',
      'analytics.view',
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-sidebar-nav-group').length).toBe(3);
    expect(element.querySelectorAll('app-sidebar-nav-item').length).toBe(7);
    expect(element.textContent).toContain('AI');
    expect(element.textContent).toContain('AI Agent');
    expect(element.textContent).toContain('Integrations');
    expect(element.textContent).toContain('Analytics');
    expect(element.textContent).not.toContain('Settings');
  });

  it('shows only Overview, Conversations, Customers, Knowledge Base for Support Engineer staff (no Settings/AI Agent/Integrations/Analytics)', async () => {
    configure([
      'overview.view',
      'conversations.view',
      'conversations.manage',
      'customers.view',
      'customers.manage',
      'knowledge_base.view',
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-sidebar-nav-group').length).toBe(2);
    expect(element.querySelectorAll('app-sidebar-nav-item').length).toBe(4);
    expect(element.textContent).toContain('Workspace');
    expect(element.textContent).toContain('Overview');
    expect(element.textContent).toContain('Conversations');
    expect(element.textContent).toContain('Customers');
    expect(element.textContent).toContain('Knowledge Base');
    expect(element.textContent).not.toContain('AI Agent');
    expect(element.textContent).not.toContain('Integrations');
    expect(element.textContent).not.toContain('Analytics');
    expect(element.textContent).not.toContain('Settings');
  });

  it('shows all navigation items for Owner', async () => {
    configure([
      'overview.view',
      'conversations.view',
      'customers.view',
      'ai_agent.view',
      'knowledge_base.view',
      'integrations.view',
      'analytics.view',
      'settings.view',
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-sidebar-nav-group').length).toBe(4);
    expect(element.querySelectorAll('app-sidebar-nav-item').length).toBe(8);
    expect(element.textContent).toContain('Settings');
  });

  it('renders sidebar without error when collapsed input is true', async () => {
    configure([
      'overview.view',
      'conversations.view',
      'customers.view',
      'ai_agent.view',
      'knowledge_base.view',
      'integrations.view',
      'analytics.view',
      'settings.view',
    ]);
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SidebarComponent);
    fixture.componentRef.setInput('collapsed', true);
    fixture.detectChanges();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.classList.contains('collapsed')).toBe(true);
    expect(element.querySelectorAll('a[aria-label]').length).toBeGreaterThan(0);
  });
});
