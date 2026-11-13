import { TestBed } from '@angular/core/testing';
import { provideZonelessChangeDetection } from '@angular/core';
import { APP_CONFIG } from '../config/app-config';
import { provideTenantStore, tenantStore, testTenant } from '../state/testing/tenant-store.testing';
import {
  RECONNECT_BASE_MS,
  RECONNECT_CAP_MS,
  reconnectDelay,
  RealtimeService,
  SseEvent,
} from './realtime.service';

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

  beforeEach(() => {
    // A stream that ends schedules a reconnect after a random delay (full jitter). Pin it near the
    // top of the range so that retry never fires inside a test that runs on real timers; the
    // tests of the delay itself set their own value.
    vi.spyOn(Math, 'random').mockReturnValue(0.999);
  });

  afterEach(() => {
    service.disconnect();
    vi.restoreAllMocks(); // the `fetch` and `Math.random` spies must not leak into the next test
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

  describe('reconnecting (MOH-65)', () => {
    /** A response whose stream ends at once: a connection that succeeded, then dropped. */
    const connectedThenDropped = () =>
      Promise.resolve(
        new Response(new ReadableStream({ start: (c) => c.close() }), {
          status: 200,
          headers: { 'content-type': 'text/event-stream' },
        }),
      );

    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('waits a random time that doubles its ceiling with every failure', async () => {
      // A random of 0.5 makes each wait half its ceiling: 500, 1000, 2000, 4000 ms.
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockImplementation(() => Promise.reject(new Error('network error')));
      service = createService();

      service.connect();
      await vi.advanceTimersByTimeAsync(0);
      expect(fetchMock).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(499);
      expect(fetchMock).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1); // t = 500
      expect(fetchMock).toHaveBeenCalledTimes(2);

      await vi.advanceTimersByTimeAsync(999);
      expect(fetchMock).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(1); // t = 1500
      expect(fetchMock).toHaveBeenCalledTimes(3);

      await vi.advanceTimersByTimeAsync(1999);
      expect(fetchMock).toHaveBeenCalledTimes(3);
      await vi.advanceTimersByTimeAsync(1); // t = 3500
      expect(fetchMock).toHaveBeenCalledTimes(4);

      await vi.advanceTimersByTimeAsync(3999);
      expect(fetchMock).toHaveBeenCalledTimes(4);
      await vi.advanceTimersByTimeAsync(1); // t = 7500
      expect(fetchMock).toHaveBeenCalledTimes(5);
    });

    it('is spread out: clients that fail together do not retry together', async () => {
      const randoms = [0.1, 0.9];
      vi.spyOn(Math, 'random').mockImplementation(() => randoms.shift() ?? 0.5);
      const times: number[] = [];
      vi.spyOn(globalThis, 'fetch').mockImplementation(() => {
        times.push(Date.now());
        return Promise.reject(new Error('down'));
      });

      for (let client = 0; client < 2; client += 1) {
        TestBed.resetTestingModule();
        const instance = createService();
        instance.connect();
        await vi.advanceTimersByTimeAsync(1_000);
        instance.disconnect();
      }

      // Same start, same failure, different first waits (100 ms and 900 ms).
      const secondAttempts = times.filter((_, i) => i % 2 === 1).map((t) => t % 1_000_000);
      expect(new Set(secondAttempts).size).toBe(secondAttempts.length);
    });

    it('starts the count over after a successful connection', async () => {
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
      const responses = [
        () => Promise.reject(new Error('down')), // attempt 0 fails: wait 500
        () => Promise.reject(new Error('down')), // attempt 1 fails: wait 1000
        connectedThenDropped, //                    connects, then drops: count resets
        () => Promise.reject(new Error('down')),
      ];
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockImplementation(() => (responses.shift() ?? (() => new Promise(() => undefined)))());
      service = createService();

      service.connect();
      await vi.advanceTimersByTimeAsync(500); // 2nd attempt
      await vi.advanceTimersByTimeAsync(1_000); // 3rd attempt: connects, then the stream ends
      expect(fetchMock).toHaveBeenCalledTimes(3);

      // Without the reset the next wait would be 2000 ms (500 with it).
      await vi.advanceTimersByTimeAsync(499);
      expect(fetchMock).toHaveBeenCalledTimes(3);
      await vi.advanceTimersByTimeAsync(1);
      expect(fetchMock).toHaveBeenCalledTimes(4);
    });

    it('does not reconnect after disconnect, even while a reconnect is waiting', async () => {
      vi.spyOn(Math, 'random').mockReturnValue(0.99);
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockImplementation(() => Promise.reject(new Error('down')));
      service = createService();

      service.connect();
      await vi.advanceTimersByTimeAsync(0); // the first attempt failed; a retry is waiting
      service.disconnect();
      await vi.advanceTimersByTimeAsync(RECONNECT_CAP_MS * 2);

      expect(fetchMock).toHaveBeenCalledTimes(1);
    });

    it('a new connect() starts from the first wait again, not from where the last left off', async () => {
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockImplementation(() => Promise.reject(new Error('down')));
      service = createService();

      service.connect();
      await vi.advanceTimersByTimeAsync(500 + 1_000 + 2_000); // four attempts, count at 3
      expect(fetchMock).toHaveBeenCalledTimes(4);

      service.connect(); // e.g. the workspace changed
      await vi.advanceTimersByTimeAsync(0);
      expect(fetchMock).toHaveBeenCalledTimes(5);
      await vi.advanceTimersByTimeAsync(500);
      expect(fetchMock).toHaveBeenCalledTimes(6);
    });

    it('reconnects after a server error status as well', async () => {
      vi.spyOn(Math, 'random').mockReturnValue(0.5);
      const fetchMock = vi
        .spyOn(globalThis, 'fetch')
        .mockImplementation(() => Promise.resolve(new Response('no', { status: 503 })));
      service = createService();

      service.connect();
      await vi.advanceTimersByTimeAsync(500);

      expect(fetchMock).toHaveBeenCalledTimes(2);
    });
  });

  describe('reconnectDelay', () => {
    it('is zero at the bottom of the range and just under the ceiling at the top', () => {
      for (let attempt = 0; attempt < 12; attempt += 1) {
        const ceiling = Math.min(RECONNECT_CAP_MS, RECONNECT_BASE_MS * 2 ** attempt);
        expect(reconnectDelay(attempt, () => 0)).toBe(0);
        expect(reconnectDelay(attempt, () => 0.999999)).toBe(Math.floor(0.999999 * ceiling));
        expect(reconnectDelay(attempt, () => 0.999999)).toBeLessThan(ceiling);
      }
    });

    it('doubles its ceiling per attempt and never passes the cap', () => {
      const top = (attempt: number) => reconnectDelay(attempt, () => 0.999999);
      expect(top(0)).toBeLessThan(1_000);
      expect(top(1)).toBeLessThan(2_000);
      expect(top(2)).toBeLessThan(4_000);
      expect(top(4)).toBeLessThan(16_000);
      expect(top(5)).toBeLessThan(RECONNECT_CAP_MS);
      for (const attempt of [6, 10, 50, 2_000]) {
        expect(top(attempt)).toBeLessThan(RECONNECT_CAP_MS);
        expect(top(attempt)).toBeGreaterThan(RECONNECT_CAP_MS - 100);
      }
    });

    it('stays inside its range for any random value', () => {
      for (let i = 0; i < 200; i += 1) {
        const delay = reconnectDelay(3);
        expect(delay).toBeGreaterThanOrEqual(0);
        expect(delay).toBeLessThan(8_000);
      }
    });
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
