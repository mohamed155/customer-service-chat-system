export interface WidgetInstance {
  id: string;
  publicId: string;
  name: string;
  displayName: string;
  primaryColor: string;
  welcomeMessage: string;
  position: 'bottom-right' | 'bottom-left';
  theme: 'light' | 'dark';
  enabled: boolean;
  allowedDomains: string[];
  createdAt: string;
  updatedAt: string;
}

export interface CreateWidgetInstancePayload {
  name: string;
  displayName?: string;
  primaryColor?: string;
  welcomeMessage?: string;
  position?: string;
  theme?: string;
  enabled?: boolean;
  allowedDomains?: string[];
}

export interface UpdateWidgetInstancePayload {
  name?: string;
  displayName?: string;
  primaryColor?: string;
  welcomeMessage?: string;
  position?: string;
  theme?: string;
  enabled?: boolean;
  allowedDomains?: string[];
}

export interface WidgetSnippet {
  snippet: string;
}
