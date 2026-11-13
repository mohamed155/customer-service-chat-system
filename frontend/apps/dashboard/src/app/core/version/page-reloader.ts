import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';

/** Loads a page from the server again. A service so a spec can replace it (jsdom cannot reload). */
@Injectable({ providedIn: 'root' })
export class PageReloader {
  private readonly document = inject(DOCUMENT);

  /** Load `url` as a new page, or the current page again when there is no URL. */
  reload(url?: string): void {
    const location = this.document.defaultView?.location;
    if (!location) return;
    if (url) location.assign(url);
    else location.reload();
  }
}
