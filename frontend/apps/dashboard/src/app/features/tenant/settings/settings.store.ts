import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';

export type SettingsTab = 'general' | 'team' | 'billing' | 'api-keys' | 'security';

export const SettingsStore = signalStore(
  withState({ activeTab: 'general' as SettingsTab }),
  withMethods((store) => ({
    setTab(activeTab: SettingsTab): void {
      patchState(store, { activeTab });
    },
  })),
);
