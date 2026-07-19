import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ComposerComponent } from './composer.component';

function createInputEvent(value: string): Event {
  const el = document.createElement('textarea');
  el.value = value;
  return { target: el } as unknown as Event;
}

describe('ComposerComponent', () => {
  let fixture: ComponentFixture<ComposerComponent>;
  let component: ComposerComponent;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ComposerComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(ComposerComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('emits sendMessage event on Enter', () => {
    const spy = vi.spyOn(component.sendMessage, 'emit');
    component.onInput(createInputEvent('hello'));
    const event = new KeyboardEvent('keydown', { key: 'Enter' });
    vi.spyOn(event, 'preventDefault');
    component.onKeydown(event);
    expect(spy).toHaveBeenCalledWith('hello');
  });

  it('does not send on Shift+Enter', () => {
    const spy = vi.spyOn(component.sendMessage, 'emit');
    component.onInput(createInputEvent('hello'));
    const event = new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true });
    component.onKeydown(event);
    expect(spy).not.toHaveBeenCalled();
  });

  it('does not send empty input', () => {
    const spy = vi.spyOn(component.sendMessage, 'emit');
    component.onInput(createInputEvent('   '));
    component.onSend();
    expect(spy).not.toHaveBeenCalled();
  });

  it('blocks input beyond 4000 characters', () => {
    const el = document.createElement('textarea');
    el.value = 'a'.repeat(4001);
    const event = { target: el } as unknown as Event;
    component.onInput(event);
    expect(el.value.length).toBe(4000);
  });
});
