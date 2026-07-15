import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ComposerComponent } from './composer.component';

describe('ComposerComponent', () => {
  function createComponent(
    overrides: {
      conversationId?: string;
      currentStatus?: string;
      submitting?: boolean;
    } = {},
  ) {
    TestBed.configureTestingModule({
      imports: [ComposerComponent],
      providers: [provideZonelessChangeDetection()],
    });
    const fixture = TestBed.createComponent(ComposerComponent);
    const component = fixture.componentInstance;
    fixture.componentRef.setInput('conversationId', overrides.conversationId ?? 'c1');
    fixture.componentRef.setInput('currentStatus', overrides.currentStatus ?? 'open');
    fixture.componentRef.setInput('submitting', overrides.submitting ?? false);
    fixture.detectChanges();
    return { fixture, component };
  }

  it('starts in reply mode with empty form', () => {
    const { component } = createComponent();
    expect(component['mode']()).toBe('reply');
    expect(component['form'].value).toEqual({ body: '' });
  });

  it('switches between all three mode tabs', () => {
    const { component, fixture } = createComponent();
    const tabs = fixture.nativeElement.querySelectorAll('.mode-tab');
    expect(tabs.length).toBe(3);
    expect(tabs[0].textContent).toContain('Reply');
    expect(tabs[1].textContent).toContain('Internal note');
    expect(tabs[2].textContent).toContain('Customer');

    tabs[1].click();
    fixture.detectChanges();
    expect(component['mode']()).toBe('note');
    expect(tabs[1].classList.contains('active')).toBe(true);
    expect(tabs[0].classList.contains('active')).toBe(false);
    expect(tabs[2].classList.contains('active')).toBe(false);

    tabs[2].click();
    fixture.detectChanges();
    expect(component['mode']()).toBe('customer');
    expect(tabs[2].classList.contains('active')).toBe(true);
    expect(tabs[0].classList.contains('active')).toBe(false);
    expect(tabs[1].classList.contains('active')).toBe(false);

    tabs[0].click();
    fixture.detectChanges();
    expect(component['mode']()).toBe('reply');
    expect(tabs[0].classList.contains('active')).toBe(true);
  });

  it('rejects whitespace-only input', () => {
    const { component, fixture } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    const textarea: HTMLTextAreaElement = fixture.nativeElement.querySelector('textarea');
    textarea.value = '   ';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const submitBtn = fixture.nativeElement.querySelector('button[type="submit"]');
    expect(submitBtn.disabled).toBe(true);

    component['submit']();
    expect(sendSpy).not.toHaveBeenCalled();
  });

  it('rejects empty input and shows validation', () => {
    const { component, fixture } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    component['form'].controls.body.markAsTouched();
    fixture.detectChanges();

    component['submit']();
    expect(sendSpy).not.toHaveBeenCalled();
  });

  it('disables submit button when submitting', () => {
    const { fixture } = createComponent({ submitting: true });
    fixture.detectChanges();

    const submitBtn = fixture.nativeElement.querySelector('button[type="submit"]');
    expect(submitBtn.disabled).toBe(true);
    expect(submitBtn.textContent).toContain('Sending');
  });

  it('emits send event with correct payload and resets form', () => {
    const { component, fixture } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    component['form'].controls.body.setValue('Hello, this is a reply');
    fixture.detectChanges();

    component['submit']();

    expect(sendSpy).toHaveBeenCalledWith({
      kind: 'reply',
      body: 'Hello, this is a reply',
    });
    expect(component['form'].value.body).toBe('');
    expect(component['mode']()).toBe('reply');
  });

  it('emits note kind when in note mode', () => {
    const { component } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    component['mode'].set('note');
    component['form'].controls.body.setValue('Internal note text');

    component['submit']();

    expect(sendSpy).toHaveBeenCalledWith({
      kind: 'note',
      body: 'Internal note text',
    });
  });

  it('emits customer kind when in customer mode', () => {
    const { component } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    component['mode'].set('customer');
    component['form'].controls.body.setValue('Customer message content');

    component['submit']();

    expect(sendSpy).toHaveBeenCalledWith({
      kind: 'customer',
      body: 'Customer message content',
    });
  });

  it('rejects whitespace-only input in all modes', () => {
    const { component } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    for (const m of ['reply', 'note', 'customer'] as const) {
      component['mode'].set(m);
      component['form'].controls.body.setValue('   ');
      component['submit']();
      expect(sendSpy).not.toHaveBeenCalled();
    }
  });

  it('handles long plain text safely', () => {
    const { component } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    const longText = 'A'.repeat(5000);
    component['form'].controls.body.setValue(longText);
    component['submit']();

    expect(sendSpy).toHaveBeenCalledWith({
      kind: 'reply',
      body: longText,
    });
  });

  it('trims whitespace from message body before emitting', () => {
    const { component } = createComponent();
    const sendSpy = vi.fn();
    component.send.subscribe(sendSpy);

    component['form'].controls.body.setValue('  Hello with spaces  ');
    component['submit']();

    expect(sendSpy).toHaveBeenCalledWith({
      kind: 'reply',
      body: 'Hello with spaces',
    });
  });

  it('uses correct placeholder text based on mode', () => {
    const { fixture, component } = createComponent();
    fixture.detectChanges();

    let textarea = fixture.nativeElement.querySelector('textarea');
    expect(textarea.placeholder).toContain('Type a reply');

    component['mode'].set('note');
    fixture.detectChanges();

    textarea = fixture.nativeElement.querySelector('textarea');
    expect(textarea.placeholder).toContain('internal note');

    component['mode'].set('customer');
    fixture.detectChanges();

    textarea = fixture.nativeElement.querySelector('textarea');
    expect(textarea.placeholder).toContain('Log a customer message');
  });
});
