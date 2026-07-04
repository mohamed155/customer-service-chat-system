import { Injectable } from '@angular/core';

type LogLevel = 'debug' | 'info' | 'warn' | 'error';

@Injectable({ providedIn: 'root' })
export class LoggerService {
  debug(context: string, message: string, data?: unknown): void {
    this.write('debug', context, message, data);
  }
  info(context: string, message: string, data?: unknown): void {
    this.write('info', context, message, data);
  }
  warn(context: string, message: string, data?: unknown): void {
    this.write('warn', context, message, data);
  }
  error(context: string, message: string, data?: unknown): void {
    this.write('error', context, message, data);
  }
  private write(level: LogLevel, context: string, message: string, data?: unknown): void {
    const entry = { timestamp: new Date().toISOString(), level, context, message, data };
    console[level](entry);
  }
}
