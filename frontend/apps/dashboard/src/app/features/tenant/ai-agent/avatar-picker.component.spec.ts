import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { AvatarPickerComponent } from './avatar-picker.component';

describe('AvatarPickerComponent', () => {
  function createComponent(presets: string[]) {
    TestBed.configureTestingModule({
      imports: [AvatarPickerComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(AvatarPickerComponent);
    fixture.componentRef.setInput('presets', presets);
    fixture.detectChanges();
    return fixture;
  }

  it('renders preset buttons and upload button', () => {
    const fixture = createComponent(['bot', 'smiley']);
    expect(fixture.nativeElement.querySelectorAll('.preset-btn').length).toBe(2);
    expect(fixture.nativeElement.textContent).toContain('Upload Image');
  });

  it('selects a preset on click', () => {
    const fixture = createComponent(['bot', 'smiley']);
    const buttons = fixture.nativeElement.querySelectorAll('.preset-btn');
    buttons[1].click();
    fixture.detectChanges();
    expect(buttons[1].classList.contains('selected')).toBe(true);
  });

  it('shows error for oversized file', () => {
    const fixture = createComponent([]);
    const fileInput = fixture.nativeElement.querySelector('input[type="file"]');
    const oversized = new File(['x'.repeat(300 * 1024)], 'test.png', { type: 'image/png' });
    Object.defineProperty(fileInput, 'files', { value: [oversized] });
    fileInput.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('smaller than 256 KB');
  });
});
