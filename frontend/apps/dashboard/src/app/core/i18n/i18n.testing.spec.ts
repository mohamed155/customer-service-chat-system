import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { TranslocoPipe, TranslocoService } from '@jsverse/transloco';
import { takeMissingKeys } from './i18n.testing';
import sharedEnglish from '../../../../public/i18n/shared/en.json';

@Component({
  imports: [TranslocoPipe],
  template: `<p>{{ 'sidebar.item.overview' | transloco }}</p>
    <p>{{ 'relativeTime.minutes' | transloco: { n: 5 } }}</p>`,
})
class KnownKeysComponent {}

@Component({
  imports: [TranslocoPipe],
  template: `<p>{{ 'sidebar.item.noSuchKey' | transloco }}</p>`,
})
class UnknownKeyComponent {}

describe('component tests with the English catalog', () => {
  it('render text from the real English catalog, on the first change detection', () => {
    const fixture = TestBed.createComponent(KnownKeysComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Overview');
    expect(fixture.nativeElement.textContent).toContain('5m ago');
  });

  // The guard of MOH-155: a template that uses a key the English catalog lacks fails its test.
  it('fail when a template uses a key the English catalog lacks', () => {
    // The pipe throws from inside a subscription, so the error comes out of a timer: hold it back.
    vi.useFakeTimers();
    try {
      const fixture = TestBed.createComponent(UnknownKeyComponent);
      fixture.detectChanges();
      vi.clearAllTimers();
    } finally {
      vi.useRealTimers();
    }
    // `test-setup.ts` would fail this test on the same list.
    expect(takeMissingKeys()).toEqual(['sidebar.item.noSuchKey']);
  });

  it('fail when code asks for a key the English catalog lacks', () => {
    const transloco = TestBed.inject(TranslocoService);
    expect(() => transloco.translate('nothing.here')).toThrow('Missing translation key');
    expect(takeMissingKeys()).toEqual(['nothing.here']);
  });

  it('have every message of the English catalog readable as ICU', () => {
    const transloco = TestBed.inject(TranslocoService);
    const walk = (node: unknown, path: string[]) => {
      for (const [key, value] of Object.entries(node as Record<string, unknown>)) {
        const here = [...path, key];
        if (typeof value === 'string') {
          // Every message that takes a value gets one, so a typo in a message fails here.
          const params = Object.fromEntries(
            [...value.matchAll(/\{(\w+)/g)].map((m) => [m[1], 'x']),
          );
          expect(() => transloco.translate(here.join('.'), params), here.join('.')).not.toThrow();
        } else {
          walk(value, here);
        }
      }
    };
    walk(sharedEnglish, []);
  });
});
