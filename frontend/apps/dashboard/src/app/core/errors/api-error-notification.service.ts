import { Injectable, signal } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class ApiErrorNotificationService {
  private readonly currentMessage = signal<string | null>(null);

  readonly message = this.currentMessage.asReadonly();

  show(message: string): void {
    this.currentMessage.set(message);
  }

  clear(): void {
    this.currentMessage.set(null);
  }
}
