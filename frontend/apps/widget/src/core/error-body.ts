import { HttpErrorResponse } from '@angular/common/http';

/** The code of a send that found its conversation closed (`409`). */
export const CONVERSATION_CLOSED = 'conversation_closed';

/**
 * The `code` an error response names, in either format the API answers in (ADR-012 gap A2): an
 * RFC 9457 problem document (`code`, `detail` and a numeric `status`) or the legacy
 * `{ error: { code, message } }` envelope. `null` for a body in neither shape (HTML from a
 * gateway, a string, no body), which the widget treats as a generic error.
 */
export function readErrorCode(error: HttpErrorResponse): string | null {
  const body: unknown = error.error;
  if (!body || typeof body !== 'object') return null;

  const legacy = (body as { error?: unknown }).error;
  if (legacy && typeof legacy === 'object') {
    const { code, message } = legacy as { code?: unknown; message?: unknown };
    if (typeof code === 'string' && typeof message === 'string') return code;
  }

  const problem = body as { code?: unknown; detail?: unknown; status?: unknown };
  if (
    typeof problem.code === 'string' &&
    typeof problem.detail === 'string' &&
    typeof problem.status === 'number'
  ) {
    return problem.code;
  }
  return null;
}

/**
 * Whether a failed send means the conversation has ended. The API answers `409` for that and for
 * nothing else on this route, so a `409` is a closed conversation unless its body names another
 * code, in either format; a `409` with no body, or one in neither shape, still is.
 */
export function isConversationClosed(error: unknown): boolean {
  if (!(error instanceof HttpErrorResponse) || error.status !== 409) return false;
  const code = readErrorCode(error);
  return code === null || code === CONVERSATION_CLOSED;
}
