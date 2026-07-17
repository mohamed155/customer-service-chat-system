import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { PromptVariable } from '../../../../core/api/ai-agent.models';
import { PreviewPanelComponent } from './preview-panel.component';

describe('PreviewPanelComponent', () => {
  @Component({
    standalone: true,
    imports: [PreviewPanelComponent],
    template: ` <app-preview-panel [content]="content()" [variables]="variables()" /> `,
    changeDetection: ChangeDetectionStrategy.OnPush,
  })
  class HostComponent {
    readonly content = signal('');
    readonly variables = signal<PromptVariable[] | null>(null);
  }

  async function setup(content: string, variables: PromptVariable[] | null) {
    TestBed.configureTestingModule({
      imports: [HostComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(HostComponent);
    const host = fixture.componentInstance;
    host.content.set(content);
    host.variables.set(variables);
    fixture.detectChanges();
    return fixture;
  }

  it('shows placeholder text when content is empty', async () => {
    const fixture = await setup('', null);
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.placeholder-text')).toBeTruthy();
    });
  });

  it('renders substituted text', async () => {
    const fixture = await setup('Hello {{agent_name}}', [
      { name: 'agent_name', description: 'The AI agent name', sample: 'Aria' },
    ]);
    await vi.waitFor(() => {
      fixture.detectChanges();
      const text = fixture.nativeElement.querySelector('.preview-text');
      expect(text).toBeTruthy();
      expect(text.textContent).toContain('Aria');
    });
  });

  it('shows error chips for unknown placeholders', async () => {
    const fixture = await setup('{{business_hours}}', [
      { name: 'agent_name', description: 'The AI agent name', sample: 'Aria' },
    ]);
    await vi.waitFor(() => {
      fixture.detectChanges();
      const chips = fixture.nativeElement.querySelectorAll('.error-chip');
      expect(chips.length).toBeGreaterThan(0);
    });
  });

  it('shows placeholder text when content is empty', async () => {
    const fixture = await setup('', []);
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.placeholder-text')).toBeTruthy();
    });
  });
});
