import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { DialogShellComponent } from './dialog-shell.component';

@Component({
  standalone: true,
  imports: [DialogShellComponent],
  template: `
    <button #trigger type="button" (click)="open.set(true)">Open</button>
    @if (open()) {
      <app-dialog-shell
        [open]="true"
        [dismissDisabled]="submitting()"
        ariaLabelledby="dialog-title"
        ariaDescribedby="dialog-desc"
        (dismiss)="close()"
      >
        <h2 id="dialog-title">Invite member</h2>
        <p id="dialog-desc">Dialog body</p>
        <input #firstField type="text" />
        <button type="button">Secondary</button>
      </app-dialog-shell>
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
class HostComponent {
  readonly open = signal(false);
  readonly submitting = signal(false);

  close(): void {
    this.open.set(false);
  }
}

describe('DialogShellComponent', () => {
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

  it('restores focus to the trigger after dismissal', async () => {
    const fixture = await setup();
    const trigger = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    trigger.focus();
    trigger.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.dialog')).toBeTruthy();
      expect(document.activeElement?.nodeName).toBe('INPUT');
    });

    fixture.nativeElement
      .querySelector('.dialog-backdrop')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.componentInstance.open()).toBe(false);
      expect(document.activeElement).toBe(trigger);
    });
  });

  it('blocks Escape and backdrop dismissal while submitting', async () => {
    const fixture = await setup();
    fixture.componentInstance.submitting.set(true);
    fixture.nativeElement.querySelector('button')?.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.dialog')).toBeTruthy();
    });

    fixture.nativeElement
      .querySelector('.dialog')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    fixture.nativeElement
      .querySelector('.dialog-backdrop')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.detectChanges();

    expect(fixture.componentInstance.open()).toBe(true);
  });
});
