import { ErrorHandler, inject, Injectable } from '@angular/core';
import { ApiError } from '../api/api.models';
import { LoggerService } from '../logging/logger.service';
const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' && error !== null && 'code' in error && 'status' in error;
@Injectable()
export class GlobalErrorHandler implements ErrorHandler {
  private readonly logger = inject(LoggerService);
  handleError(error: unknown): void {
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
