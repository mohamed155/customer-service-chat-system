import { Injectable, inject } from '@angular/core';
import { Title } from '@angular/platform-browser';
import { RouterStateSnapshot, TitleStrategy } from '@angular/router';
import { TranslocoService } from '@jsverse/transloco';

/** `pages.overview.title`: letters and digits in dot-separated parts, no spaces. */
const CATALOG_KEY = /^[a-z][A-Za-z0-9]*(?:\.[A-Za-z0-9_]+)+$/;

/**
 * Sets the browser tab's title from the route's `title`. A title that is a catalog key
 * (`PAGE_TITLES.overview.title`) is written in the active language; any other text is used as it
 * is, which is how routes that have not moved to the catalog yet keep working.
 */
@Injectable({ providedIn: 'root' })
export class TranslatedTitleStrategy extends TitleStrategy {
  private readonly title = inject(Title);
  private readonly transloco = inject(TranslocoService);

  override updateTitle(snapshot: RouterStateSnapshot): void {
    const title = this.buildTitle(snapshot);
    if (title === undefined) return;
    this.title.setTitle(CATALOG_KEY.test(title) ? this.transloco.translate(title) : title);
  }
}
