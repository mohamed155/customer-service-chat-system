import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { WidgetInstance } from '../../../core/api/widget.models';
import { ApiError } from '../../../core/api/api.models';
import { WidgetEditorComponent } from './widget-editor.component';

describe('WidgetEditorComponent', () => {
  async function render(form: Partial<WidgetInstance>, error: ApiError | null = null) {
    await TestBed.configureTestingModule({
      imports: [WidgetEditorComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();
    const fixture = TestBed.createComponent(WidgetEditorComponent);
    fixture.componentRef.setInput('form', form);
    fixture.componentRef.setInput('isNew', true);
    fixture.componentRef.setInput('error', error);
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  it('titles the panel and labels the fields from the catalog', async () => {
    const el = await render({ name: 'Support' });
    expect(el.querySelector('h3')?.textContent?.trim()).toBe('New Widget');
    for (const text of ['Display name', 'Primary color', 'Welcome message', 'Allowed domains']) {
      expect(el.textContent).toContain(text);
    }
    expect(el.querySelector<HTMLInputElement>('#wgt-name')?.placeholder).toBe('My Widget');
  });

  it('says what is wrong with a field, with the limit as a number for the language', async () => {
    const el = await render({
      name: 'x'.repeat(81),
      welcomeMessage: 'y'.repeat(501),
      primaryColor: 'blue',
      allowedDomains: ['not a domain'],
    });
    const errors = Array.from(el.querySelectorAll('.field-error')).map((e) =>
      e.textContent?.trim(),
    );
    expect(errors).toEqual([
      'Name must be 80 characters or fewer',
      'Must be a valid hex color (e.g. #0066FF)',
      'Message must be 500 characters or fewer',
      'Invalid domain: not a domain',
    ]);
  });

  it('asks for a name', async () => {
    const el = await render({ name: '   ' });
    expect(el.querySelector('.field-error')?.textContent?.trim()).toBe('Name is required');
  });

  it('words a refused save by its status, with the reference, never the API text', async () => {
    const el = await render(
      { name: 'Support' },
      {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        requestId: 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f',
      },
    );
    const banner = el.querySelector('.error-banner');
    expect(banner?.textContent).toContain('Some settings are not valid.');
    expect(banner?.textContent).not.toContain('Validation failed');
    expect(banner?.querySelector('[data-testid="reference"]')?.textContent).toBe('3c4d5e6f');
  });
});
