import { describe, expect, it } from 'vitest';
import { CHANNEL_BREAKDOWN } from './analytics.fixtures';
import { CONVERSATION_FIXTURES } from './conversation.fixtures';
import { CUSTOMER_FIXTURES } from './customer.fixtures';

describe('fixture integrity', () => {
  it('resolves every conversation customerId', () => {
    const customerIds = new Set(CUSTOMER_FIXTURES.map((customer) => customer.id));

    expect(
      CONVERSATION_FIXTURES.every((conversation) => customerIds.has(conversation.customerId)),
    ).toBe(true);
  });

  it('covers conversation channel, status, and sentiment variants', () => {
    expect(new Set(CONVERSATION_FIXTURES.map((conversation) => conversation.channel))).toEqual(
      new Set(['web', 'whatsapp', 'telegram']),
    );
    expect(new Set(CONVERSATION_FIXTURES.map((conversation) => conversation.status))).toEqual(
      new Set(['open', 'escalated', 'closed']),
    );
    expect(new Set(CONVERSATION_FIXTURES.map((conversation) => conversation.sentiment))).toEqual(
      new Set(['positive', 'neutral', 'angry']),
    );
  });

  it('covers every channel and status combination', () => {
    const combinations = new Set(
      CONVERSATION_FIXTURES.map((conversation) => `${conversation.channel}:${conversation.status}`),
    );

    expect(combinations).toEqual(
      new Set([
        'web:open',
        'web:escalated',
        'web:closed',
        'whatsapp:open',
        'whatsapp:escalated',
        'whatsapp:closed',
        'telegram:open',
        'telegram:escalated',
        'telegram:closed',
      ]),
    );
  });

  it('keeps conversation messages usable for AI and escalation visuals', () => {
    for (const conversation of CONVERSATION_FIXTURES) {
      expect(conversation.messages.length).toBeGreaterThanOrEqual(3);
      expect(conversation.messages.some((message) => message.author === 'ai')).toBe(true);
      expect(
        conversation.messages
          .filter((message) => message.author === 'ai')
          .every(
            (message) => typeof message.aiConfidence === 'number' && !!message.citations?.length,
          ),
      ).toBe(true);
    }

    expect(
      CONVERSATION_FIXTURES.filter((conversation) => conversation.status === 'escalated').every(
        (conversation) => conversation.messages.some((message) => message.author === 'system'),
      ),
    ).toBe(true);
  });

  it('keeps channel breakdown at 100 percent', () => {
    expect(CHANNEL_BREAKDOWN.reduce((sum, item) => sum + item.percentage, 0)).toBe(100);
  });
});
