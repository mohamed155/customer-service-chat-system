import { PAGE_TITLES } from './page-title';

describe('PAGE_TITLES', () => {
  it('has exact title/subtitle text for every static entry', () => {
    expect(PAGE_TITLES.conversations).toEqual({
      title: 'Conversations',
      subtitle: 'Manage your team conversations',
    });
    expect(PAGE_TITLES.customers).toEqual({
      title: 'Customers',
      subtitle: 'Customer profiles and conversation history',
    });
    expect(PAGE_TITLES.aiAgent).toEqual({
      title: 'AI Agent',
      subtitle: 'Configure how your assistant behaves',
    });
    expect(PAGE_TITLES.knowledgeBase).toEqual({
      title: 'Knowledge Base',
      subtitle: 'Train your AI with trusted company knowledge',
    });
    expect(PAGE_TITLES.integrations).toEqual({
      title: 'Integrations',
      subtitle: 'Connect channels and business systems',
    });
    expect(PAGE_TITLES.analytics).toEqual({
      title: 'Analytics',
      subtitle: 'Trends across every channel',
    });
    expect(PAGE_TITLES.settings).toEqual({
      title: 'Settings',
      subtitle: 'Workspace preferences and security',
    });
    expect(PAGE_TITLES.platform).toEqual({
      title: 'Platform',
      subtitle: 'Platform administration',
    });
  });

  it('computes the overview subtitle fresh from the current date rather than baking it in at module load', () => {
    const { title, subtitle } = PAGE_TITLES.overview;
    expect(title).toBe('Overview');
    expect(typeof subtitle).toBe('function');
    const resolve = subtitle as () => string;

    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date('2026-06-16T12:00:00'));
      expect(resolve()).toBe('Tuesday, June 16 · Your support cockpit');

      vi.setSystemTime(new Date('2026-12-25T12:00:00'));
      expect(resolve()).toBe('Friday, December 25 · Your support cockpit');
    } finally {
      vi.useRealTimers();
    }
  });
});
