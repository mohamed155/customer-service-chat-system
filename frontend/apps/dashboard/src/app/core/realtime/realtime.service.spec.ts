import { TestBed } from '@angular/core/testing';
import { provideZonelessChangeDetection } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';
import { provideTenantStore, tenantStore, testTenant } from '../state/testing/tenant-store.testing';
import { RealtimeService, SseEvent } from './realtime.service';

describe('RealtimeService', () => {
  let service: RealtimeService;

  function createService() {
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        RealtimeService,
        provideTenantStore(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: 'http://localhost:8080/api/v1' } },
      ],
    });
    return TestBed.inject(RealtimeService);
  }

  afterEach(() => {
    service.disconnect();
  });

  it('includes credentials and X-Tenant-ID header on the fetch', async () => {
    let fetchUrl = '';
    let fetchOpts: RequestInit = {};
    vi.spyOn(globalThis, 'fetch').mockImplementation((url, opts) => {
      fetchUrl = url as string;
      fetchOpts = opts ?? {};
      return Promise.resolve(
        new Response(null, { status: 200, headers: { 'content-type': 'text/event-stream' } }),
      );
    });

    service = createService();
    tenantStore().activate(testTenant('tenant-42'));
    service.connect();

    await vi.waitFor(() => {
      expect(fetchUrl).toContain('/tenant/events');
    });

    const headers = fetchOpts.headers as Record<string, string>;
    expect(headers['X-Tenant-ID']).toBe('tenant-42');
    expect(fetchOpts.credentials).toBe('include');
  });

  it('parses text/event-stream frames into typed events', async () => {
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          encoder.encode('event: escalation.assigned\nid: 1\ndata: {"escalationId":"e1"}\n\n'),
        );
        controller.enqueue(
          encoder.encode('event: escalation.queued\nid: 2\ndata: {"escalationId":"e2"}\n\n'),
        );
        controller.close();
      },
    });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } }),
    );

    service = createService();
    const events: SseEvent[] = [];
    service.events().subscribe((e) => events.push(e));
    service.connect();

    await vi.waitFor(() => {
      expect(events.length).toBe(2);
    });

    expect(events[0].event).toBe('escalation.assigned');
    expect(events[0].data).toBe('{"escalationId":"e1"}');
    expect(events[1].event).toBe('escalation.queued');
    expect(events[1].data).toBe('{"escalationId":"e2"}');
  });

  it('ignores :ping comment frames', async () => {
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode(': ping\n\n'));
        controller.enqueue(encoder.encode('event: escalation.assigned\nid: 1\ndata: {}\n\n'));
        controller.close();
      },
    });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } }),
    );

    service = createService();
    const events: SseEvent[] = [];
    service.events().subscribe((e) => events.push(e));
    service.connect();

    await vi.waitFor(() => {
      expect(events.length).toBe(1);
    });

    expect(events[0].event).toBe('escalation.assigned');
  });

  it('reconnects on connection loss', async () => {
    vi.useFakeTimers();
    let attempts = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => {
      attempts++;
      return Promise.reject(new Error('network error'));
    });

    service = createService();
    service.connect();
    // initial attempt at t=0 + retry at t=1000ms
    await vi.advanceTimersByTimeAsync(1000);
    expect(attempts).toBe(2);

    // each retry schedules the next at retryDelay (reset to 1000 by disconnect)
    await vi.advanceTimersByTimeAsync(500);
    expect(attempts).toBe(2); // no timer fired yet

    await vi.advanceTimersByTimeAsync(500);
    expect(attempts).toBe(3); // retry at t=2000

    vi.useRealTimers();
  });

  it('resets parser state cleanly per (re)subscription', async () => {
    let fetchCount = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => {
      fetchCount++;
      const encoder = new TextEncoder();
      const stream = new ReadableStream({
        start(controller) {
          if (fetchCount === 1) {
            controller.enqueue(
              encoder.encode('event: escalation.assigned\nid: 1\ndata: {"e":"1"}\n\n'),
            );
          } else {
            controller.enqueue(
              encoder.encode('event: escalation.queued\nid: 2\ndata: {"e":"2"}\n\n'),
            );
          }
          controller.close();
        },
      });
      return Promise.resolve(
        new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } }),
      );
    });

    service = createService();
    const events: SseEvent[] = [];
    service.events().subscribe((e) => events.push(e));
    service.connect();
    await vi.waitFor(() => expect(events.length).toBe(1));
    expect(events[0].event).toBe('escalation.assigned');

    service.disconnect();
    events.length = 0;
    service.connect();
    await vi.waitFor(() => expect(events.length).toBe(1));
    expect(events[0].event).toBe('escalation.queued');
  });
  describe('a workspace change (MOH-61)', () => {
    const openStreams: { tenant: string | undefined; signal: AbortSignal }[] = [];

    beforeEach(() => {
      openStreams.length = 0;
      vi.spyOn(globalThis, 'fetch').mockImplementation((_url, opts) => {
        const headers = (opts?.headers ?? {}) as Record<string, string>;
        openStreams.push({ tenant: headers['X-Tenant-ID'], signal: opts?.signal as AbortSignal });
        // A stream that stays open until it is aborted.
        return new Promise(() => undefined);
      });
    });

    it('reopens the stream for the new workspace and aborts the old one', () => {
      service = createService();
      const tenants = tenantStore();
      tenants.activate(testTenant('tenant-a'));
      service.connect();
      expect(openStreams.map((s) => s.tenant)).toEqual(['tenant-a']);

      tenants.switchTo(testTenant('tenant-b'));

      expect(openStreams.map((s) => s.tenant)).toEqual(['tenant-a', 'tenant-b']);
      expect(openStreams[0].signal.aborted).toBe(true);
      expect(openStreams[1].signal.aborted).toBe(false);
    });

    it('closes the stream when the workspace is cleared', () => {
      service = createService();
      const tenants = tenantStore();
      tenants.activate(testTenant('tenant-a'));
      service.connect();

      tenants.clear();

      expect(openStreams).toHaveLength(1);
      expect(openStreams[0].signal.aborted).toBe(true);
    });

    it('does not open a stream by itself when none was connected', () => {
      service = createService();
      const tenants = tenantStore();
      tenants.activate(testTenant('tenant-a'));

      tenants.switchTo(testTenant('tenant-b'));

      expect(openStreams).toHaveLength(0);
    });
  });
});
