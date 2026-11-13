import { PAGE_TITLES } from './page-title';

describe('PAGE_TITLES', () => {
  it('holds catalog keys for every entry', () => {
    for (const [name, entry] of Object.entries(PAGE_TITLES)) {
      expect(entry.title).toBe(`pages.${name}.title`);
      expect(entry.subtitle).toBe(`pages.${name}.subtitle`);
    }
  });

  it('gives the overview subtitle the date, computed fresh rather than baked in at module load', () => {
    const { subtitleParams } = PAGE_TITLES.overview;
    expect(subtitleParams).toBeDefined();

    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-06-16T12:00:00'));
      expect(subtitleParams?.('en')).toEqual({ date: 'Tuesday, June 16' });

      vi.setSystemTime(new Date('2026-12-25T12:00:00'));
      expect(subtitleParams?.('en')).toEqual({ date: 'Friday, December 25' });
    } finally {
      vi.useRealTimers();
    }
  });

  it('writes the overview date in the locale it is given', () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-06-16T12:00:00'));
      expect(PAGE_TITLES.overview.subtitleParams?.('de')).toEqual({ date: 'Dienstag, 16. Juni' });
    } finally {
      vi.useRealTimers();
    }
  });
});
