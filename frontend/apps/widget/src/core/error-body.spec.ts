import { HttpErrorResponse } from '@angular/common/http';
import { isConversationClosed, readErrorCode } from './error-body';

const legacy = (code: string) => ({
  error: { code, message: 'raw', details: [], request_id: 'r' },
});
const problem = (status: number, code: string) => ({
  type: `urn:problem:${code}`,
  title: 'T',
  status,
  detail: 'raw',
  instance: '/widget/v1/x',
  code,
  request_id: 'r',
  errors: [],
});
const failed = (status: number, error: unknown) => new HttpErrorResponse({ status, error });

describe('readErrorCode', () => {
  it('reads the code from the legacy envelope and from a problem document', () => {
    expect(readErrorCode(failed(409, legacy('conversation_closed')))).toBe('conversation_closed');
    expect(readErrorCode(failed(409, problem(409, 'conversation_closed')))).toBe(
      'conversation_closed',
    );
  });

  it('reads nothing from a body in neither shape', () => {
    for (const body of [
      '<html>Bad gateway</html>',
      '',
      null,
      [1],
      {},
      { message: 'x' },
      { code: 'x' },
      // Looks like a problem but has no status or no detail: not trusted.
      { code: 'x', detail: 'y' },
      { code: 'x', status: 400 },
      { error: 'a string' },
      { error: { code: 1, message: 2 } },
    ]) {
      expect(readErrorCode(failed(400, body))).toBeNull();
    }
  });
});

describe('isConversationClosed', () => {
  it('is a 409 in either format', () => {
    expect(isConversationClosed(failed(409, legacy('conversation_closed')))).toBe(true);
    expect(isConversationClosed(failed(409, problem(409, 'conversation_closed')))).toBe(true);
  });

  it('is still a 409 with no body or a body in neither shape', () => {
    expect(isConversationClosed(new HttpErrorResponse({ status: 409 }))).toBe(true);
    expect(isConversationClosed(failed(409, '<html>Conflict</html>'))).toBe(true);
  });

  it('is not a 409 that names another code', () => {
    expect(isConversationClosed(failed(409, legacy('conflict')))).toBe(false);
    expect(isConversationClosed(failed(409, problem(409, 'conflict')))).toBe(false);
  });

  it('is not any other status or any other value', () => {
    for (const status of [400, 401, 404, 422, 429, 500, 503]) {
      expect(isConversationClosed(failed(status, legacy('conversation_closed')))).toBe(false);
      expect(isConversationClosed(failed(status, problem(status, 'conversation_closed')))).toBe(
        false,
      );
    }
    expect(isConversationClosed(new Error('409'))).toBe(false);
    expect(isConversationClosed(null)).toBe(false);
  });
});
