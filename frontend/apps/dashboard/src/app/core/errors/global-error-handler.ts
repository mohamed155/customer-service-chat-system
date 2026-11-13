import { ErrorHandler, inject, Injectable } from '@angular/core';
import { ApiError } from '../api/api.models';
import { LoggerService } from '../logging/logger.service';
import { ClientErrorReporter } from './client-error-reporter';
const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' && error !== null && 'code' in error && 'status' in error;
@Injectable()
export class GlobalErrorHandler implements ErrorHandler {
  private readonly logger = inject(LoggerService);
  private readonly reporter = inject(ClientErrorReporter);
  handleError(error: unknown): void {
    // Everything that gets here nobody handled: it goes to the backend too (MOH-168).
    this.reporter.report(error);
    if (isApiError(error)) {
      this.logger.error('api', error.code, { status: error.status, requestId: error.requestId });
      return;
    }
    this.logger.error(
      'application',
      error instanceof Error ? error.message : 'Unhandled error',
      error,
    );
  }
}
