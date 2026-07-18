import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ToolResultViewerComponent } from './tool-result-viewer.component';

describe('ToolResultViewerComponent', () => {
  function createComponent(result?: unknown, error?: string) {
    TestBed.configureTestingModule({
      imports: [ToolResultViewerComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ToolResultViewerComponent);
    fixture.componentRef.setInput('result', result);
    fixture.componentRef.setInput('error', error);
    fixture.detectChanges();
    return fixture;
  }

  it('renders nothing when both result and error are absent', () => {
    const fixture = createComponent(undefined, undefined);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent?.trim()).toBe('');
  });

  it('renders toggle button when result is present', () => {
    const fixture = createComponent({ data: 'test' }, undefined);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Show details');
  });

  it('renders toggle button when error is present', () => {
    const fixture = createComponent(undefined, 'Something went wrong');
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Show details');
  });

  it('expands to show result on click', () => {
    const fixture = createComponent({ key: 'value' }, undefined);
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector('button')!;
    button.click();
    fixture.detectChanges();
    expect(el.textContent).toContain('"key"');
    expect(el.textContent).toContain('"value"');
    expect(el.textContent).toContain('Hide details');
  });

  it('expands to show error on click', () => {
    const fixture = createComponent(undefined, 'API timeout');
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector('button')!;
    button.click();
    fixture.detectChanges();
    expect(el.textContent).toContain('API timeout');
    expect(el.textContent).toContain('Error:');
  });

  it('shows both result and error when both present', () => {
    const fixture = createComponent({ ok: true }, 'Partial failure');
    const el = fixture.nativeElement as HTMLElement;
    const button = el.querySelector('button')!;
    button.click();
    fixture.detectChanges();
    expect(el.textContent).toContain('Partial failure');
    expect(el.textContent).toContain('"ok"');
    expect(el.textContent).toContain('Result:');
    expect(el.textContent).toContain('Error:');
  });
});
