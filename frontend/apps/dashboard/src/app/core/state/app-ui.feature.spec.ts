import { appUiActions, appUiFeature, createInitialAppUiState } from './app-ui.feature';

describe('appUi feature', () => {
  beforeEach(() => localStorage.clear());

  it('updates theme and sidebar state', () => {
    let state = appUiFeature.reducer(
      createInitialAppUiState(),
      appUiActions.themeModeChanged({ themeMode: 'dark' }),
    );
    expect(state.themeMode).toBe('dark');
    state = appUiFeature.reducer(state, appUiActions.sidebarToggled());
    expect(state.sidebarCollapsed).toBe(true);
    state = appUiFeature.reducer(state, appUiActions.sidebarCollapsedSet({ collapsed: false }));
    expect(state.sidebarCollapsed).toBe(false);
  });

  it('falls back to system for invalid persisted data', () => {
    localStorage.setItem('app.themeMode', 'invalid');
    expect(createInitialAppUiState().themeMode).toBe('system');
  });

  it('selects expected slices', () => {
    const root = { appUi: { themeMode: 'light' as const, sidebarCollapsed: true } };
    expect(appUiFeature.selectThemeMode(root)).toBe('light');
    expect(appUiFeature.selectSidebarCollapsed(root)).toBe(true);
  });
});
