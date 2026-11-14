import { marker } from './marker';
import { ownValue } from './own-value';

/** Catalog keys (shared catalog) of the channel names. */
const CHANNEL_KEYS: Readonly<Record<string, string>> = {
  web: marker('channels.web'),
  web_chat: marker('channels.web_chat'),
  whatsapp: marker('channels.whatsapp'),
  telegram: marker('channels.telegram'),
  mobile_sdk: marker('channels.mobile_sdk'),
  email: marker('channels.email'),
  phone: marker('channels.phone'),
  widget: marker('channels.widget'),
};

/** The catalog key of a channel's name, or `null` for a channel this app has no name for. */
export function channelLabelKey(channel: string): string | null {
  return ownValue(CHANNEL_KEYS, channel) ?? null;
}
