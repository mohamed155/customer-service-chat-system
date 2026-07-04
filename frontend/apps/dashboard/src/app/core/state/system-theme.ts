import { computed, Signal, signal } from '@angular/core';
import { ThemeMode } from './app-ui.feature';

export type ResolvedTheme = 'light' | 'dark';

export const selectResolvedTheme = (themeMode: Signal<ThemeMode>): Signal<ResolvedTheme> => {
  const systemDark = signal(false);
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    systemDark.set(media.matches);
    media.addEventListener('change', (event) => systemDark.set(event.matches));
  }
  return computed(() => {
    const mode = themeMode();
    return mode === 'system' ? (systemDark() ? 'dark' : 'light') : mode;
  });
};
