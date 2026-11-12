import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ErrorReferenceComponent } from './error-reference.component';

const ID = 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f';

describe('ErrorReferenceComponent', () => {
  const writeText = vi.fn();

  beforeEach(() => {
    writeText.mockReset();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    TestBed.configureTestingModule({
      imports: [ErrorReferenceComponent],
      providers: [provideZonelessChangeDetection()],
    });
  });

  const create = () => {
    const fixture = TestBed.createComponent(ErrorReferenceComponent);
    fixture.componentRef.setInput('requestId', ID);
    fixture.detectChanges();
    return fixture;
  };

  it('shows the short reference, not the whole ID', () => {
    const el: HTMLElement = create().nativeElement;
    expect(el.querySelector('[data-testid="reference"]')?.textContent).toBe('3c4d5e6f');
    expect(el.textContent).not.toContain('0197f2b4');
  });

  it('copies the full request ID', async () => {
    writeText.mockResolvedValue(undefined);
    const fixture = create();

    (fixture.nativeElement as HTMLElement).querySelector('button')!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(writeText).toHaveBeenCalledWith(ID);
    expect(fixture.nativeElement.querySelector('button').textContent).toContain('Copied');
  });

  it('says so when copying is not possible', async () => {
    writeText.mockRejectedValue(new Error('denied'));
    const fixture = create();

    (fixture.nativeElement as HTMLElement).querySelector('button')!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('button').textContent).toContain('Copy failed');
  });
});
