import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';

/** Loads a page from the server again. A service so a spec can replace it (jsdom cannot reload). */
@Injectable({ providedIn: 'root' })
export class PageReloader {
  private readonly document = inject(DOCUMENT);

  /** Load `url` as a new page, or the current page again when there is no URL. */
  reload(url?: string): void {
    const view = this.document.defaultView;
    const location = view?.location;
    if (!view || !location) return;
    if (!url) {
      location.reload();
      return;
    }
    const target = new URL(url, location.href);
    const withoutFragment = (u: string | URL) => String(u).split('#')[0];
    // `hash` is empty for both no fragment and an empty one (`/x#`); only the second jumps in place.
    const hasFragment = target.href.includes('#');
    if (hasFragment && withoutFragment(target) === withoutFragment(location.href)) {
      // `assign` to the same document with a fragment is a jump within the page, not a load: take
      // over the target's fragment, then load the document again.
      view.history.replaceState(view.history.state, '', target.href);
      location.reload();
      return;
    }
    location.assign(target.href);
  }
}
