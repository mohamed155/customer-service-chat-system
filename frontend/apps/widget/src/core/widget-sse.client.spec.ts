import { WidgetSseClient } from './widget-sse.client';
import { WidgetEvent } from './models';

function streamOf(...chunks: string[]): Response {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      // Stay open, like a live SSE connection.
    },
  });
  return new Response(body, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

describe('WidgetSseClient', () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  async function firstEvents(count: number, ...chunks: string[]): Promise<WidgetEvent[]> {
    globalThis.fetch = (() => Promise.resolve(streamOf(...chunks))) as typeof fetch;
    const client = new WidgetSseClient();
    return new Promise((resolve) => {
      const seen: WidgetEvent[] = [];
      const sub = client.stream('/events', 'token').subscribe((ev) => {
        seen.push(ev);
        if (seen.length === count) {
          sub.unsubscribe();
          resolve(seen);
        }
      });
    });
  }

  it('uses the event name as the type of a message.created payload', async () => {
    const message = { id: 'm1', sender: 'assistant', body: 'Hi there', createdAt: '2026-01-01' };
    const [event] = await firstEvents(
      1,
      `event: message.created\ndata: ${JSON.stringify({ message })}\nid: 1\n\n`,
    );
    expect(event).toEqual({ type: 'message.created', message } as WidgetEvent);
  });

  it('uses the event name as the type of an ai.delta payload', async () => {
    const [event] = await firstEvents(
      1,
      'event: ai.delta\ndata: {"messageId":null,"text":"Hel"}\nid: 1\n\n',
    );
    expect(event).toEqual({ type: 'ai.delta', messageId: null, text: 'Hel' });
  });

  it('reads events that arrive split across chunks, in order', async () => {
    const events = await firstEvents(
      2,
      'event: ai.delta\ndata: {"messageId":null,"te',
      'xt":"A"}\n\nevent: conversation.updated\n',
      'data: {"handling":"human","teamOnline":true}\n\n',
    );
    expect(events.map((e) => e.type)).toEqual(['ai.delta', 'conversation.updated']);
  });

  it('ignores a data line with no event name and malformed JSON', async () => {
    const events = await firstEvents(
      1,
      'data: {"text":"orphan"}\n\nevent: ai.delta\ndata: not-json\n\nevent: ai.delta\ndata: {"text":"ok","messageId":null}\n\n',
    );
    expect(events).toEqual([{ type: 'ai.delta', text: 'ok', messageId: null }]);
  });
});
