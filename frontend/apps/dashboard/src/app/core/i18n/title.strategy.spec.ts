import { Title } from '@angular/platform-browser';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { Component } from '@angular/core';
import { TranslatedTitleStrategy } from './title.strategy';
import { TitleStrategy } from '@angular/router';

@Component({ template: '' })
class Page {}

describe('TranslatedTitleStrategy', () => {
  async function titleAfterVisiting(path: string) {
    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          { path: 'key', component: Page, title: 'pages.customers.title' },
          { path: 'plain', component: Page, title: 'Sign in' },
          { path: 'none', component: Page },
        ]),
        { provide: TitleStrategy, useExisting: TranslatedTitleStrategy },
      ],
    });
    document.title = 'before';
    await TestBed.inject(Router).navigateByUrl(path);
    return TestBed.inject(Title).getTitle();
  }

  it('writes a catalog key in the active language', async () => {
    expect(await titleAfterVisiting('/key')).toBe('Customers');
  });

  it('uses text that is not a key as it is, for routes not moved to the catalog yet', async () => {
    expect(await titleAfterVisiting('/plain')).toBe('Sign in');
  });

  it('leaves the title alone when the route has none', async () => {
    expect(await titleAfterVisiting('/none')).toBe('before');
  });
});
