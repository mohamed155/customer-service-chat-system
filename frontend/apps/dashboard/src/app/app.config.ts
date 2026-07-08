import {
  ApplicationConfig,
  ErrorHandler,
  provideAppInitializer,
  inject,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
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
import { TenantContextEffects } from './core/state/tenant-context.effects';
import { tenantContextFeature } from './core/state/tenant-context.feature';
import { tenantContextInterceptor } from './core/http/tenant-context.interceptor';
import { devIdentityInterceptor } from './core/http/dev-identity.interceptor';
import { CurrentUserService } from './core/tenant/current-user.service';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    provideHttpClient(
      withFetch(),
      withInterceptors([
        authTokenInterceptor,
        tenantContextInterceptor,
        devIdentityInterceptor,
        apiErrorInterceptor,
      ]),
    ),
    provideTaiga(),
    provideStore({
      [appUiFeature.name]: appUiFeature.reducer,
      [tenantContextFeature.name]: tenantContextFeature.reducer,
    }),
    provideEffects(AppUiEffects, TenantContextEffects),
    provideAppInitializer(() => {
      const cus = inject(CurrentUserService);
      return cus.load();
    }),
    ...(environment.enableNgRxDevtools ? [provideStoreDevtools({ maxAge: 25 })] : []),
    { provide: APP_CONFIG, useValue: environment },
    { provide: ErrorHandler, useClass: GlobalErrorHandler },
  ],
};
