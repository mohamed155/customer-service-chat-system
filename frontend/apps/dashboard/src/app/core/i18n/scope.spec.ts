import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { TRANSLOCO_LOADER, TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { of } from 'rxjs';
import sharedEnglish from '../../../../public/i18n/shared/en.json';

@Component({
  imports: [TranslocoPipe],
  providers: [provideTranslocoScope('knowledge')],
  template: `<h1>{{ 'knowledge.title' | transloco }}</h1>
    <p>{{ 'sidebar.item.overview' | transloco }}</p>
    <i>{{ 'knowledge.count' | transloco: { n: 2 } }}</i>`,
})
class AreaComponent {}

// How a feature area uses a scope (frontend/CLAUDE.md, "A scope"): its catalog is the file
// `<scope>/<locale>.json`, its keys are written `<scope>.<key>`, and the shared catalog stays
// available beside it.
describe('a feature scope', () => {
  it('reads <scope>/en and keeps the shared catalog', () => {
    const asked: string[] = [];
    TestBed.overrideProvider(TRANSLOCO_LOADER, {
      useValue: {
        getTranslation: (path: string) => {
          asked.push(path);
          return of(
            path === 'knowledge/en'
              ? { title: 'Articles', count: '{n, plural, one {# article} other {# articles}}' }
              : path === 'en'
                ? sharedEnglish
                : {},
          );
        },
      },
    });
    const fixture = TestBed.createComponent(AreaComponent);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Articles');
    expect(fixture.nativeElement.textContent).toContain('2 articles');
    expect(asked).toContain('knowledge/en');
  });
});
