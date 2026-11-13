import { TestBed } from '@angular/core/testing';
import { LoggerService } from '../logging/logger.service';
import { ClientErrorReporter } from './client-error-reporter';
import { GlobalErrorHandler } from './global-error-handler';

describe('GlobalErrorHandler', () => {
  const logger = { error: vi.fn() };
  const reporter = { report: vi.fn() };

  beforeEach(() => {
    logger.error.mockReset();
    reporter.report.mockReset();
    TestBed.configureTestingModule({
      providers: [
        GlobalErrorHandler,
        { provide: LoggerService, useValue: logger },
        { provide: ClientErrorReporter, useValue: reporter },
      ],
    });
  });

  it('logs the request ID of an error that came from an API call (MOH-64)', () => {
    TestBed.inject(GlobalErrorHandler).handleError({
      code: 'internal_error',
      message: 'x',
      status: 500,
      requestId: 'req_0123',
    });

    expect(logger.error).toHaveBeenCalledWith('api', 'internal_error', {
      status: 500,
      requestId: 'req_0123',
    });
  });

  it('logs any other error as an application error, without a request ID', () => {
    const failure = new Error('boom');
    TestBed.inject(GlobalErrorHandler).handleError(failure);

    expect(logger.error).toHaveBeenCalledWith('application', 'boom', failure);
  });

  it('reports every error it handles to the backend (MOH-168), API errors included', () => {
    const failure = new Error('boom');
    const apiError = { code: 'internal_error', message: 'x', status: 500, requestId: 'req_0123' };
    const handler = TestBed.inject(GlobalErrorHandler);

    handler.handleError(failure);
    handler.handleError(apiError);

    expect(reporter.report).toHaveBeenNthCalledWith(1, failure);
    expect(reporter.report).toHaveBeenNthCalledWith(2, apiError);
  });
});
