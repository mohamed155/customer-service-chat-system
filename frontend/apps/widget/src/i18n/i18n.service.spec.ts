import { TestBed } from '@angular/core/testing';
import englishFile from '../../public/i18n/en.json';
import { I18n } from './i18n.service';
import { EN, MessageKey } from './messages';

const jsonResponse = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('I18n', () => {
  let i18n: I18n;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    i18n = TestBed.inject(I18n);
  });

  afterEach(() => vi.unstubAllGlobals());

  it('answers in English with no loading', () => {
    expect(i18n.t('chat.close')).toBe('Close chat');
    expect(i18n.locale()).toBe('en');
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('fills in parameters and leaves an unknown placeholder alone', () => {
    expect(i18n.t('rating.group', { value: 4 })).toBe('Rating: 4 out of 5');
    expect(i18n.t('rating.group')).toBe('Rating: {value} out of 5');
  });

  it('picks the plural form for the count', () => {
    expect(i18n.t('rating.star', { count: 1 })).toBe('1 star');
    expect(i18n.t('rating.star', { count: 3 })).toBe('3 stars');
    expect(i18n.t('rating.star', { count: 0 })).toBe('0 stars');
  });

  it('returns the key for a key that exists nowhere', () => {
    expect(i18n.t('no.such.key' as MessageKey)).toBe('no.such.key');
  });

  describe('another locale', () => {
    it('loads i18n/<locale>.json beside the widget and uses it', async () => {
      fetchMock.mockResolvedValue(jsonResponse({ 'chat.close': 'Chat schließen' }));
      await i18n.use('de', 'https://cdn.example/widget/');
      expect(String(fetchMock.mock.calls[0][0])).toBe('https://cdn.example/widget/i18n/de.json');
      expect(i18n.t('chat.close')).toBe('Chat schließen');
      expect(i18n.locale()).toBe('de');
    });

    it('falls back to English for a key the translation does not have', async () => {
      fetchMock.mockResolvedValue(jsonResponse({ 'chat.close': 'Chat schließen' }));
      await i18n.use('de');
      expect(i18n.t('chat.retry')).toBe('Try again');
    });

    it('uses the plural rules of the locale', async () => {
      fetchMock.mockResolvedValue(
        jsonResponse({
          'rating.star': { one: '{count} Stern', other: '{count} Sterne' },
        }),
      );
      await i18n.use('de');
      expect(i18n.t('rating.star', { count: 1 })).toBe('1 Stern');
      expect(i18n.t('rating.star', { count: 2 })).toBe('2 Sterne');
    });

    it('tries the language when the region has no file', async () => {
      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 404))
        .mockResolvedValueOnce(jsonResponse({ 'chat.close': 'Fechar chat' }));
      await i18n.use('pt_br');
      expect(
        fetchMock.mock.calls.map((c) => new URL(String(c[0])).pathname.split('/').pop()),
      ).toEqual(['pt-BR.json', 'pt.json']);
      expect(i18n.t('chat.close')).toBe('Fechar chat');
      expect(i18n.locale()).toBe('pt');
    });

    it('stays in English when the file is missing, unreachable or not a catalog', async () => {
      fetchMock.mockResolvedValue(jsonResponse({}, 404));
      await i18n.use('fr');
      expect(i18n.locale()).toBe('en');
      fetchMock.mockRejectedValue(new Error('offline'));
      await i18n.use('fr');
      expect(i18n.locale()).toBe('en');
      expect(i18n.t('chat.close')).toBe('Close chat');
      fetchMock.mockResolvedValue(jsonResponse([1, 2, 3]));
      await i18n.use('es');
      expect(i18n.t('chat.close')).toBe('Close chat');
    });

    it('ignores entries that are not text or plural forms: a catalog is data, not code', async () => {
      fetchMock.mockResolvedValue(
        jsonResponse({ 'chat.close': { evil: 1 }, 'chat.retry': 42, 'chat.error': 'Fehler' }),
      );
      await i18n.use('de');
      expect(i18n.t('chat.close')).toBe('Close chat');
      expect(i18n.t('chat.retry')).toBe('Try again');
      expect(i18n.t('chat.error')).toBe('Fehler');
    });

    it('does not fetch for English or for something that is not a locale tag', async () => {
      await i18n.use('en-US');
      await i18n.use('');
      await i18n.use('../../etc/passwd');
      await i18n.use(null);
      expect(fetchMock).not.toHaveBeenCalled();
      expect(i18n.locale()).toBe('en');
    });

    it('goes back to English', async () => {
      fetchMock.mockResolvedValue(jsonResponse({ 'chat.close': 'Chat schließen' }));
      await i18n.use('de');
      await i18n.use('en');
      expect(i18n.t('chat.close')).toBe('Close chat');
    });
  });

  it('ships an English file that matches the bundled catalog, for translators to copy', () => {
    expect(englishFile).toEqual(JSON.parse(JSON.stringify(EN)));
  });
});
