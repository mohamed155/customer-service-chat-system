import { inject, Injectable, Injector, signal } from '@angular/core';
import { ApiError } from '../api/api.models';
import { mapHttpError } from '../errors/http-error.mapper';
import { MfaCredential, MfaService, mfaErrorKey } from './mfa.service';

/**
 * Asks the user for a fresh code when the server says a sensitive action needs one
 * (`mfa_step_up_required`). The prompt (a dialog in the app root) is shared: requests that need a
 * code while it is open wait for the same answer.
 */
@Injectable({ providedIn: 'root' })
export class StepUpService {
  // Resolved when a code is actually submitted, so the prompt that sits in the app root does not
  // pull the HTTP stack into every place that mounts the root component.
  private readonly injector = inject(Injector);
  private waiting: Array<(proven: boolean) => void> = [];

  readonly open = signal(false);
  readonly pending = signal(false);
  /** Catalog key of what went wrong with the last code, or `null`. */
  readonly errorKey = signal<string | null>(null);

  /** Show the prompt. Resolves `true` once a code was proven, `false` if the user cancels. */
  prompt(): Promise<boolean> {
    return new Promise((resolve) => {
      this.waiting.push(resolve);
      this.open.set(true);
    });
  }

  async submit(credential: MfaCredential): Promise<void> {
    this.pending.set(true);
    this.errorKey.set(null);
    try {
      await this.injector.get(MfaService).stepUp(credential);
      this.settle(true);
    } catch (error) {
      const apiError = isApiError(error) ? error : mapHttpError(error);
      this.errorKey.set(mfaErrorKey(apiError.status));
    } finally {
      this.pending.set(false);
    }
  }

  cancel(): void {
    this.settle(false);
  }

  private settle(proven: boolean): void {
    this.open.set(false);
    this.errorKey.set(null);
    const waiting = this.waiting;
    this.waiting = [];
    waiting.forEach((resolve) => resolve(proven));
  }
}

const isApiError = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  typeof (error as ApiError).code === 'string' &&
  typeof (error as ApiError).status === 'number';
