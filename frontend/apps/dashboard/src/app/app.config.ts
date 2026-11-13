import {
  ApplicationConfig,
  ErrorHandler,
  provideAppInitializer,
  inject,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { provideHttpClient, withFetch, withInterceptors } from '@angular/common/http';
import {
  provideRouter,
  withComponentInputBinding,
  withNavigationErrorHandler,
  withPreloading,
} from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { provideEffects } from '@ngrx/effects';
import { provideStore } from '@ngrx/store';
import { environment } from '../environments/environment';
import { APP_CONFIG } from './core/config/app-config';
import { routes } from './app.routes';
import { AppUiEffects } from './core/state/app-ui.effects';
import { appUiFeature } from './core/state/app-ui.feature';
import { authTokenInterceptor } from './core/http/auth-token.interceptor';
import { apiErrorInterceptor } from './core/http/api-error.interceptor';
import { requestIdInterceptor } from './core/http/request-id.interceptor';
import { httpResilienceInterceptor } from './core/http/http-resilience.interceptor';
import { ClientErrorReporter } from './core/errors/client-error-reporter';
import { GlobalErrorHandler } from './core/errors/global-error-handler';
import { TenantContextEffects } from './core/state/tenant-context.effects';
import { devtoolsProviders } from './core/state/devtools';
import { tenantContextFeature } from './core/state/tenant-context.feature';
import { tenantContextInterceptor } from './core/http/tenant-context.interceptor';
import { devIdentityInterceptor } from './core/http/dev-identity.interceptor';
import { stepUpInterceptor } from './core/http/step-up.interceptor';
import { CurrentUserService } from './core/tenant/current-user.service';
import { AppUpdateService } from './core/version/app-update.service';
import { recoverFromChunkLoadError } from './core/version/chunk-recovery';
import { IdlePreloadStrategy } from './core/router/idle-preload.strategy';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(
      routes,
      withComponentInputBinding(),
      // A lazy route whose files a deploy removed: load the page again (MOH-167).
      withNavigationErrorHandler(recoverFromChunkLoadError),
      // Routes marked `data: { preload: true }` load when the browser is idle (MOH-171).
      withPreloading(IdlePreloadStrategy),
    ),
    provideHttpClient(
      withFetch(),
      withInterceptors([
        // First, so the ID is on the request every later interceptor and the error handler see.
        requestIdInterceptor,
        authTokenInterceptor,
        tenantContextInterceptor,
        devIdentityInterceptor,
        // Outside apiErrorInterceptor on purpose: it sees the mapped error, and it retries once.
        stepUpInterceptor,
        apiErrorInterceptor,
        // Closest to the network, inside apiErrorInterceptor: it sees raw HTTP errors for the retry
        // rules, and a timeout it raises is mapped to an ApiError on the way out.
        httpResilienceInterceptor,
      ]),
    ),
    // Taiga's text-scale watcher writes HTML into a hidden iframe, which Trusted Types blocks and a
    // policy cannot cover (the iframe is its own context). See frontend/CLAUDE.md, Trusted Types.
    provideTaiga({ fontScaling: false }),
    provideStore({
      [appUiFeature.name]: appUiFeature.reducer,
      [tenantContextFeature.name]: tenantContextFeature.reducer,
    }),
    provideEffects(AppUiEffects, TenantContextEffects),
    // Watches for a newer release while the page stays open (MOH-167). Does not block start-up.
    provideAppInitializer(() => inject(AppUpdateService).start()),
    // Listens for page hide and unhandled rejections, to report browser errors (MOH-168).
    provideAppInitializer(() => inject(ClientErrorReporter).start()),
    provideAppInitializer(() => {
      const cus = inject(CurrentUserService);
      return cus.load();
    }),
    ...devtoolsProviders,
    { provide: APP_CONFIG, useValue: environment },
    { provide: ErrorHandler, useClass: GlobalErrorHandler },
  ],
};
