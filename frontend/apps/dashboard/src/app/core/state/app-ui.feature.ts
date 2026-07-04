import {
  createActionGroup,
  createFeature,
  createReducer,
  emptyProps,
  on,
  props,
} from '@ngrx/store';

export type ThemeMode = 'light' | 'dark' | 'system';

export interface AppUiState {
  readonly themeMode: ThemeMode;
  readonly sidebarCollapsed: boolean;
}

const isThemeMode = (value: string | null): value is ThemeMode =>
  value === 'light' || value === 'dark' || value === 'system';

export const createInitialAppUiState = (): AppUiState => {
  const stored =
    typeof localStorage !== 'undefined' && typeof localStorage.getItem === 'function'
      ? localStorage.getItem('app.themeMode')
      : null;
  return { themeMode: isThemeMode(stored) ? stored : 'system', sidebarCollapsed: false };
};

export const appUiActions = createActionGroup({
  source: 'App UI',
  events: {
    'Theme Mode Changed': props<{ themeMode: ThemeMode }>(),
    'Sidebar Toggled': emptyProps(),
    'Sidebar Collapsed Set': props<{ collapsed: boolean }>(),
  },
});

export const appUiFeature = createFeature({
  name: 'appUi',
  reducer: createReducer(
    createInitialAppUiState(),
    on(appUiActions.themeModeChanged, (state, { themeMode }) => ({ ...state, themeMode })),
    on(appUiActions.sidebarToggled, (state) => ({
      ...state,
      sidebarCollapsed: !state.sidebarCollapsed,
    })),
    on(appUiActions.sidebarCollapsedSet, (state, { collapsed }) => ({
      ...state,
      sidebarCollapsed: collapsed,
    })),
  ),
});

export const { selectThemeMode, selectSidebarCollapsed } = appUiFeature;
