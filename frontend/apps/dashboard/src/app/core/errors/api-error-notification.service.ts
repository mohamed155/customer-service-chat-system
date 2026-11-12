import { Injectable, signal } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class ApiErrorNotificationService {
  private readonly currentMessage = signal<string | null>(null);
  private readonly currentRequestId = signal<string | null>(null);

  readonly message = this.currentMessage.asReadonly();
  /** The request ID of the call that caused the message, when there is one (MOH-64). */
  readonly requestId = this.currentRequestId.asReadonly();

  show(message: string, requestId?: string): void {
    this.currentMessage.set(message);
    this.currentRequestId.set(requestId || null);
  }

  clear(): void {
    this.currentMessage.set(null);
    this.currentRequestId.set(null);
  }
}
