import { bootstrapApplication } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideZonelessChangeDetection } from '@angular/core';
import { AppComponent } from './app.component';
import { WIDGET_API_BASE } from './core/widget-api.service';

bootstrapApplication(AppComponent, {
  providers: [
    provideZonelessChangeDetection(),
    provideHttpClient(),
    // The widget is served from the same origin as the public /widget/v1 API
    // (the loader fetches its config from that origin too).
    { provide: WIDGET_API_BASE, useValue: window.location.origin },
  ],
}).catch(console.error);
