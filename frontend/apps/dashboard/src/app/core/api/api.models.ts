import type { components } from './generated/api';

// What is here has no generated twin, except `ApiErrorDetail`, which follows the contract's
// `ErrorDetail` (its `code` and `message`) and is looser on purpose: a detail without a `field` is
// accepted. `ApiError` flattens the error envelope and adds the HTTP status, `PaginatedResponse` is
// the normalised form of the wire's pagination block, and `ApiResponse` and `ApiListQuery` are the
// app's own request and response shapes.

export interface ApiResponse<T> {
  readonly data: T;
  readonly requestId?: string;
}
export type ApiErrorDetail = Omit<components['schemas']['ErrorDetail'], 'field'> & {
  readonly field?: string;
};
export interface ApiError {
  readonly code: string;
  readonly message: string;
  readonly details?: ApiErrorDetail[];
  readonly requestId?: string;
  readonly status: number;
}
export interface PaginatedResponse<T> {
  readonly items: T[];
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
}
export interface ApiListQuery {
  readonly limit?: number;
  readonly cursor?: string;
  readonly sort?: string;
  readonly order?: 'asc' | 'desc';
  readonly q?: string;
}
