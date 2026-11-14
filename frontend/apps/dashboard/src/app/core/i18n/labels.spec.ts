import { channelLabelKey } from './labels';

describe('channelLabelKey', () => {
  it('gives the catalog key of a channel the app has a name for', () => {
    expect(channelLabelKey('web_chat')).toBe('channels.web_chat');
    expect(channelLabelKey('whatsapp')).toBe('channels.whatsapp');
  });

  it('gives none for a channel it has no name for, and never reads a prototype property', () => {
    expect(channelLabelKey('carrier_pigeon')).toBeNull();
    expect(channelLabelKey('constructor')).toBeNull();
    expect(channelLabelKey('__proto__')).toBeNull();
  });
});
