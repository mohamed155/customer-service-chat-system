export interface WidgetConfig {
  widgetId: string;
  displayName: string;
  primaryColor: string;
  welcomeMessage: string;
  position: 'bottom-right' | 'bottom-left';
  theme: 'light' | 'dark';
  enabled: boolean;
}

export interface WidgetConversation {
  id: string;
  handling: 'ai' | 'human' | 'closed';
  teamOnline: boolean;
  endedNote: boolean;
  messages: WidgetMessage[];
}

export interface WidgetMessage {
  id: string;
  sender: 'visitor' | 'assistant' | 'agent' | 'system';
  senderDisplayName?: string;
  body: string;
  createdAt: string;
}

export interface SessionResponse {
  sessionToken: string;
  expiresAt: string;
}

export type WidgetEvent =
  | { type: 'message.created'; message: WidgetMessage }
  | { type: 'ai.delta'; text: string; messageId: string | null }
  | { type: 'conversation.updated'; handling: string; teamOnline: boolean };

export class RateLimitedError extends Error {
  constructor() {
    super('Rate limited');
  }
}

export class SessionExpiredError extends Error {
  constructor() {
    super('Session expired');
  }
}

export interface WidgetFeedback {
  rating: number;
  comment: string | null;
  submittedAt: string;
}

export interface PendingFeedback {
  conversationId: string;
  endedAt: string;
}

export interface SubmitFeedbackPayload {
  rating: number;
  comment?: string | null;
}
