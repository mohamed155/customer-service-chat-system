export interface ApiResponse<T> {
  readonly data: T;
  readonly requestId?: string;
}
export interface ApiErrorDetail {
  readonly field?: string;
  readonly code: string;
  readonly message: string;
}
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
