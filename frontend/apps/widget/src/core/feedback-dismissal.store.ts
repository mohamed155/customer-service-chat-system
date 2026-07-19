import { Injectable } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class FeedbackDismissalStore {
  private readonly storagePrefix = 'hx_widget_feedback_dismissed_';
  private readonly dismissed: Set<string> = new Set();

  private readStorage(conversationId: string): boolean {
    try {
      return localStorage.getItem(this.storagePrefix + conversationId) === 'true';
    } catch {
      return this.dismissed.has(conversationId);
    }
  }

  private writeStorage(conversationId: string): void {
    try {
      localStorage.setItem(this.storagePrefix + conversationId, 'true');
    } catch {
      this.dismissed.add(conversationId);
    }
  }

  isDismissed(conversationId: string): boolean {
    return this.readStorage(conversationId);
  }

  dismiss(conversationId: string): void {
    this.writeStorage(conversationId);
  }
}
