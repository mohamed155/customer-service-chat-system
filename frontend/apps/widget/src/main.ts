import { bootstrapApplication } from '@angular/platform-browser';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { inject, provideAppInitializer, provideZonelessChangeDetection } from '@angular/core';
import { AppComponent } from './app.component';
import { problemAcceptInterceptor } from './core/problem-accept.interceptor';
import { WIDGET_API_BASE } from './core/widget-api.service';
import { I18n } from './i18n/i18n.service';

bootstrapApplication(AppComponent, {
  providers: [
    provideZonelessChangeDetection(),
    // Errors are asked for as problem+json, with the legacy envelope as the fallback (MOH-200).
    provideHttpClient(withInterceptors([problemAcceptInterceptor])),
    // The language is `?lang=<locale>` on the widget URL, else the browser's. English is bundled;
    // another locale is `i18n/<locale>.json` beside the widget (ADR-016).
    provideAppInitializer(() =>
      inject(I18n).use(new URLSearchParams(location.search).get('lang') ?? navigator.language),
    ),
    // The widget is served from the same origin as the public /widget/v1 API
    // (the loader fetches its config from that origin too).
    { provide: WIDGET_API_BASE, useValue: window.location.origin },
  ],
}).catch(console.error);
