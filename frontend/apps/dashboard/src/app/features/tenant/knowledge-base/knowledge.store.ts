import { computed, inject } from '@angular/core';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { EMPTY, finalize, interval, pipe, switchMap, takeWhile, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import {
  CreateItemPayload,
  ItemFilters,
  KnowledgeCategory,
  KnowledgeItemDetail,
  KnowledgeItemSummary,
  SetStatusPayload,
  UpdateItemPayload,
} from '../../../core/api/knowledge.models';
import { KnowledgeApiService } from './knowledge-api.service';

interface KnowledgeState {
  items: KnowledgeItemSummary[];
  selectedItem: KnowledgeItemDetail | null;
  categories: KnowledgeCategory[];
  filters: ItemFilters;
  cursor: string | null;
  hasMore: boolean;
  loading: boolean;
  saving: boolean;
  error: string | null;
  pollingActive: boolean;
}

const initialState: KnowledgeState = {
  items: [],
  selectedItem: null,
  categories: [],
  filters: {},
  cursor: null,
  hasMore: false,
  loading: false,
  saving: false,
  error: null,
  pollingActive: false,
};

export const KnowledgeStore = signalStore(
  withState(initialState),
  withComputed((store) => ({
    hasItems: computed(() => store.items().length > 0),
    selectedCategoryName: computed(() => {
      const selectedItem = store.selectedItem();
      if (!selectedItem?.categoryId) return null;
      return store.categories().find((c) => c.id === selectedItem.categoryId)?.name ?? null;
    }),
    hasNonTerminalIndexStatus: computed(() =>
      store
        .items()
        .some(
          (item) =>
            item.indexStatus?.status === 'pending' || item.indexStatus?.status === 'indexing',
        ),
    ),
  })),
  withMethods((store, api = inject(KnowledgeApiService)) => ({
    startPolling: rxMethod<void>(
      pipe(
        switchMap(() => {
          if (!store.hasNonTerminalIndexStatus()) return EMPTY;
          patchState(store, { pollingActive: true });
          return interval(5000).pipe(
            switchMap(() =>
              api.listItems(store.filters(), store.cursor() ?? undefined).pipe(
                tap({
                  next: (res) =>
                    patchState(store, {
                      items: res.data.items,
                      cursor: res.data.nextCursor,
                      hasMore: res.data.hasMore,
                    }),
                  error: () => {},
                }),
              ),
            ),
            takeWhile(() => store.hasNonTerminalIndexStatus()),
          );
        }),
        finalize(() => patchState(store, { pollingActive: false })),
      ),
    ),
    stopPolling(): void {
      patchState(store, { pollingActive: false });
    },
  })),
  withMethods((store, api = inject(KnowledgeApiService)) => ({
    loadList: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          api.listItems(store.filters(), store.cursor() ?? undefined).pipe(
            tap({
              next: (res) => {
                patchState(store, {
                  items: res.data.items,
                  cursor: res.data.nextCursor,
                  hasMore: res.data.hasMore,
                  loading: false,
                });
                store.startPolling();
              },
              error: (err: unknown) =>
                patchState(store, { loading: false, error: (err as ApiError).message }),
            }),
          ),
        ),
      ),
    ),
    loadMore: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true })),
        switchMap(() => {
          const cursor = store.cursor();
          if (!cursor) {
            patchState(store, { loading: false });
            return EMPTY;
          }
          return api.listItems(store.filters(), cursor).pipe(
            tap({
              next: (res) =>
                patchState(store, {
                  items: [...store.items(), ...res.data.items],
                  cursor: res.data.nextCursor,
                  hasMore: res.data.hasMore,
                  loading: false,
                }),
              error: (err: unknown) =>
                patchState(store, { loading: false, error: (err as ApiError).message }),
            }),
          );
        }),
      ),
    ),
    setFilter(filters: Partial<ItemFilters>): void {
      const merged = { ...store.filters(), ...filters };
      patchState(store, {
        filters: merged,
        cursor: null,
        loading: true,
        error: null,
      });
      api.listItems(merged).subscribe({
        next: (res) =>
          patchState(store, {
            items: res.data.items,
            cursor: res.data.nextCursor,
            hasMore: res.data.hasMore,
            loading: false,
          }),
        error: (err: unknown) =>
          patchState(store, { loading: false, error: (err as ApiError).message }),
      });
    },
    loadItem(id: string): void {
      patchState(store, { loading: true, error: null, selectedItem: null });
      api.getItem(id).subscribe({
        next: (res) => patchState(store, { selectedItem: res.data, loading: false }),
        error: (err: unknown) =>
          patchState(store, { loading: false, error: (err as ApiError).message }),
      });
    },
    createItem(payload: CreateItemPayload): void {
      patchState(store, { saving: true, error: null });
      api.createItem(payload).subscribe({
        next: (res) => patchState(store, { selectedItem: res.data, saving: false }),
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
    updateItem(id: string, payload: UpdateItemPayload): void {
      patchState(store, { saving: true, error: null });
      api.updateItem(id, payload).subscribe({
        next: (res) => patchState(store, { selectedItem: res.data, saving: false }),
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
    reindex(id: string): void {
      patchState(store, { saving: true, error: null });
      api.reindex(id).subscribe({
        next: (res) => {
          const current = store.items();
          const idx = current.findIndex((i) => i.id === id);
          if (idx !== -1) {
            const updated = [...current];
            updated[idx] = { ...updated[idx], indexStatus: res.data.indexStatus };
            patchState(store, { items: updated, saving: false });
          } else {
            patchState(store, { saving: false });
          }
          const selectedItem = store.selectedItem();
          if (selectedItem?.id === id) {
            patchState(store, {
              selectedItem: { ...selectedItem, indexStatus: res.data.indexStatus },
            });
          }
          store.startPolling();
        },
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },

    setStatus(id: string, payload: SetStatusPayload): void {
      patchState(store, { saving: true, error: null });
      api.setStatus(id, payload).subscribe({
        next: (res) => {
          if (res.data.changed) {
            const updatedItems = store
              .items()
              .map((item) =>
                item.id === id
                  ? { ...item, status: res.data.status, updatedAt: res.data.updatedAt }
                  : item,
              );
            const updatedSelected =
              store.selectedItem()?.id === id
                ? {
                    ...store.selectedItem()!,
                    status: res.data.status,
                    updatedAt: res.data.updatedAt,
                  }
                : store.selectedItem();
            patchState(store, {
              items: updatedItems,
              selectedItem: updatedSelected,
              saving: false,
            });
          } else {
            patchState(store, { saving: false });
          }
        },
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
  })),
  withMethods((store, api = inject(KnowledgeApiService)) => ({
    loadCategories: rxMethod<void>(
      pipe(
        switchMap(() =>
          api.listCategories().pipe(
            tap({
              next: (res) => patchState(store, { categories: res.data }),
              error: () => {},
            }),
          ),
        ),
      ),
    ),
  })),
  withMethods((store, api = inject(KnowledgeApiService)) => ({
    createCategory(name: string): void {
      patchState(store, { saving: true, error: null });
      api.createCategory({ name }).subscribe({
        next: () => {
          patchState(store, { saving: false });
          store.loadCategories();
        },
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
    renameCategory(id: string, name: string): void {
      patchState(store, { saving: true, error: null });
      api.renameCategory(id, { name }).subscribe({
        next: () => {
          patchState(store, { saving: false });
          store.loadCategories();
        },
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
    deleteCategory(id: string): void {
      patchState(store, { saving: true, error: null });
      api.deleteCategory(id).subscribe({
        next: () => {
          patchState(store, { saving: false });
          store.loadCategories();
        },
        error: (err: unknown) =>
          patchState(store, { saving: false, error: (err as ApiError).message }),
      });
    },
  })),
  withHooks({
    onInit(store) {
      store.loadList();
      store.loadCategories();
    },
  }),
);
