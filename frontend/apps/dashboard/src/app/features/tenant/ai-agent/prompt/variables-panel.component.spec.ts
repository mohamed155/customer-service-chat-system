import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { PromptVariable } from '../../../../core/api/ai-agent.models';
import { VariablesPanelComponent } from './variables-panel.component';

describe('VariablesPanelComponent', () => {
  const mockVariables: PromptVariable[] = [
    { name: 'agent_name', description: 'The AI agent name', sample: 'Aria' },
    { name: 'tenant_name', description: 'The tenant business name', sample: 'Acme Support' },
  ];

  @Component({
    standalone: true,
    imports: [VariablesPanelComponent],
    template: `
      <app-variables-panel [variables]="variables()" (insertVariable)="inserted.set($event)" />
    `,
    changeDetection: ChangeDetectionStrategy.OnPush,
  })
  class HostComponent {
    readonly variables = signal(mockVariables);
    readonly inserted = signal('');
  }

  async function setup() {
    TestBed.configureTestingModule({
      imports: [HostComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(HostComponent);
    fixture.detectChanges();
    return fixture;
  }

  it('renders variable chips', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.variable-chip')).toBeTruthy();
    });
    const chips = fixture.nativeElement.querySelectorAll('.variable-chip');
    expect(chips.length).toBe(2);
  });

  it('shows variable name and sample', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const chip = fixture.nativeElement.querySelector('.variable-chip');
      expect(chip.textContent).toContain('agent_name');
      expect(chip.textContent).toContain('Aria');
    });
  });

  it('emits insertVariable on click', async () => {
    const fixture = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const chip = fixture.nativeElement.querySelector('.variable-chip');
      if (chip) chip.click();
    });
    fixture.detectChanges();
    await vi.waitFor(() => {
      const host = fixture.componentInstance;
      expect(host.inserted()).toBe('agent_name');
    });
  });
});
