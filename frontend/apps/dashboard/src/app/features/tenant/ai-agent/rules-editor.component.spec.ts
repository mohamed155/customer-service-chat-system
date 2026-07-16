import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { RulesEditorComponent } from './rules-editor.component';

describe('RulesEditorComponent', () => {
  function createComponent() {
    TestBed.configureTestingModule({
      imports: [RulesEditorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(RulesEditorComponent);
    fixture.detectChanges();
    return fixture;
  }

  it('renders both sections', () => {
    const fixture = createComponent();
    expect(fixture.nativeElement.textContent).toContain('Business Rules');
    expect(fixture.nativeElement.textContent).toContain('Escalation Rules');
  });

  it('can add and remove a business rule', () => {
    const fixture = createComponent();
    const addBtn = Array.from(
      fixture.nativeElement.querySelectorAll('app-button') as NodeListOf<HTMLElement>,
    ).find((b) => b.textContent?.trim() === '+ Add rule')!;
    addBtn.querySelector('button')?.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelectorAll('.rule-row').length).toBe(1);
  });

  it('can add and remove an escalation rule', () => {
    const fixture = createComponent();
    const addBtn = Array.from(
      fixture.nativeElement.querySelectorAll('app-button') as NodeListOf<HTMLElement>,
    ).find((b) => b.textContent?.trim() === '+ Add escalation rule')!;
    addBtn.querySelector('button')?.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelectorAll('fieldset.escalation-rule').length).toBe(1);
  });

  it('shows broken skill reference warnings', () => {
    TestBed.configureTestingModule({
      imports: [RulesEditorComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(RulesEditorComponent);
    fixture.componentRef.setInput('escalationRules', [
      {
        name: 'test',
        trigger: 'human_request',
        keywords: [],
        requiredSkillIds: [],
        brokenSkillRefs: ['skill-1', 'skill-2'],
      },
    ]);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Unknown skill reference');
    expect(fixture.nativeElement.textContent).toContain('skill-1');
    expect(fixture.nativeElement.textContent).toContain('skill-2');
  });
});
