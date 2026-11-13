import { TUI_GERMAN_LANGUAGE } from '@taiga-ui/i18n/languages/german';
import { TAIGA_LANGUAGE_CODES, loadTaigaLanguage } from './taiga-language';

describe('Taiga languages', () => {
  it('loads the text of a language by its base language', async () => {
    expect(await loadTaigaLanguage('de-AT')).toBe(TUI_GERMAN_LANGUAGE);
    expect(await loadTaigaLanguage('de')).toBe(TUI_GERMAN_LANGUAGE);
  });

  it('has none for a language Taiga does not ship', () => {
    expect(loadTaigaLanguage('tlh')).toBeNull();
    expect(loadTaigaLanguage('en')).toBeNull();
  });

  it('loads every language it lists, each with its month names', async () => {
    for (const code of TAIGA_LANGUAGE_CODES) {
      const language = await loadTaigaLanguage(code);
      expect(language?.name, code).toBeTruthy();
      expect(language?.months, code).toHaveLength(12);
    }
  });
});
