export function integrationWebhookUrl(slug: string, webhookUrl: string | null): string | null {
  if (!webhookUrl) return null;
  if (slug === 'whatsapp') {
    const token = webhookUrl.split('/').pop() ?? '';
    return `/integrations/whatsapp/webhook/${token}`;
  }
  return webhookUrl;
}
