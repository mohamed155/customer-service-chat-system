export const APP_PATHS = {
  root: '',
  auth: {
    base: 'auth',
    login: 'login',
    signup: 'signup',
    forgotPassword: 'forgot-password',
    verifyEmail: 'verify-email',
  },
  platform: { base: 'platform', overviewPlaceholder: 'overview-placeholder' },
  tenant: {
    base: 'tenant',
    overview: 'overview',
    conversations: 'conversations',
    customers: 'customers',
    aiAgent: 'ai-agent',
    knowledgeBase: 'knowledge-base',
    integrations: 'integrations',
    analytics: 'analytics',
    settings: 'settings',
  },
  notFound: '**',
} as const;
