import { HttpContextToken, HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { catchError, from, switchMap, throwError } from 'rxjs';
import { ApiError } from '../api/api.models';
import { MFA_STEP_UP_REQUIRED } from '../auth/mfa.service';
import { StepUpService } from '../auth/step-up.service';

/** Marks a request that was already retried after a step-up, so it is retried only once. */
const STEP_UP_RETRIED = new HttpContextToken<boolean>(() => false);

/**
 * A `401 mfa_step_up_required` opens the code prompt, and if the user proves a code the original
 * request is sent once more. It sits outside `apiErrorInterceptor`, so it sees the mapped error.
 */
export const stepUpInterceptor: HttpInterceptorFn = (request, next) => {
  const stepUp = inject(StepUpService);

  return next(request).pipe(
    catchError((error: unknown) => {
      const apiError = error as Partial<ApiError> | null;
      const needsStepUp =
        apiError?.status === 401 &&
        apiError.code === MFA_STEP_UP_REQUIRED &&
        !request.context.get(STEP_UP_RETRIED);
      if (!needsStepUp) {
        return throwError(() => error);
      }
      return from(stepUp.prompt()).pipe(
        switchMap((proven) =>
          proven
            ? next(request.clone({ context: request.context.set(STEP_UP_RETRIED, true) }))
            : throwError(() => error),
        ),
      );
    }),
  );
};
