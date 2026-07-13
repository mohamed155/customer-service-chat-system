import { InjectionToken } from '@angular/core';

export interface AppConfig {
  readonly apiBaseUrl: string;
  readonly appName: string;
  readonly environmentName: 'development' | 'production';
  readonly enableNgRxDevtools: boolean;
  readonly publicDashboardUrl: string;
}

export const APP_CONFIG = new InjectionToken<AppConfig>('APP_CONFIG');
