import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ChoiceGroupComponent } from './choice-group.component';

describe('ChoiceGroupComponent', () => {
  it('exposes its accessible name on a semantic group', async () => {
    TestBed.configureTestingModule({
      imports: [ChoiceGroupComponent],
      providers: [provideZonelessChangeDetection()],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ChoiceGroupComponent);
    fixture.componentRef.setInput('ariaLabel', "Change Alice's role");
    fixture.componentRef.setInput('options', [{ value: 'agent', label: 'Support Agent' }]);
    fixture.componentRef.setInput('value', 'agent');
    fixture.detectChanges();

    const group = fixture.nativeElement.querySelector('[role="group"]');
    expect(group).toBeTruthy();
    expect(group.getAttribute('aria-label')).toBe("Change Alice's role");
    expect(group.querySelector('button')).toBeTruthy();
  });
});
