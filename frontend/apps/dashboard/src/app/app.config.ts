import { ApplicationConfig, ErrorHandler, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideHttpClient, withFetch, withInterceptors } from '@angular/common/http';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { provideEffects } from '@ngrx/effects';
import { provideStore } from '@ngrx/store';
import { provideStoreDevtools } from '@ngrx/store-devtools';
import { environment } from '../environments/environment';
import { APP_CONFIG } from './core/config/app-config';
import { routes } from './app.routes';
import { AppUiEffects } from './core/state/app-ui.effects';
import { appUiFeature } from './core/state/app-ui.feature';
import { authTokenInterceptor } from './core/http/auth-token.interceptor';
import { apiErrorInterceptor } from './core/http/api-error.interceptor';
import { GlobalErrorHandler } from './core/errors/global-error-handler';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    provideHttpClient(withFetch(), withInterceptors([authTokenInterceptor, apiErrorInterceptor])),
    provideTaiga(),
    provideStore({ [appUiFeature.name]: appUiFeature.reducer }),
    provideEffects(AppUiEffects),
    ...(environment.enableNgRxDevtools ? [provideStoreDevtools({ maxAge: 25 })] : []),
    { provide: APP_CONFIG, useValue: environment },
    { provide: ErrorHandler, useClass: GlobalErrorHandler },
  ],
};
