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
import { catchError, EMPTY, pipe, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import {
  WidgetInstance,
  CreateWidgetInstancePayload,
  UpdateWidgetInstancePayload,
} from '../../../core/api/widget.models';
import { WidgetApiService } from './widget-api.service';

interface WidgetsState {
  instances: WidgetInstance[];
  selectedId: string | null;
  formState: Partial<WidgetInstance> | null;
  snippet: string | null;
  loading: boolean;
  saving: boolean;
  error: string | null;
}

const initialState: WidgetsState = {
  instances: [],
  selectedId: null,
  formState: null,
  snippet: null,
  loading: false,
  saving: false,
  error: null,
};

export const WidgetsStore = signalStore(
  withState(initialState),
  withComputed((store) => ({
    hasInstances: computed(() => store.instances().length > 0),
    selectedInstance: computed(() => {
      const id = store.selectedId();
      if (!id) return null;
      return store.instances().find((i) => i.id === id) ?? null;
    }),
  })),
  withMethods((store, api = inject(WidgetApiService)) => ({
    loadList: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          api.list().pipe(
            tap({
              next: (res) => patchState(store, { instances: res.data, loading: false }),
              error: (err: unknown) =>
                patchState(store, {
                  loading: false,
                  error: (err as ApiError).message,
                }),
            }),
            catchError(() => EMPTY),
          ),
        ),
      ),
    ),
    selectInstance(id: string | null): void {
      patchState(store, {
        selectedId: id,
        formState: id ? { ...(store.instances().find((i) => i.id === id) ?? {}) } : null,
        snippet: null,
      });
      if (id) {
        api.getSnippet(id).subscribe({
          next: (res) => patchState(store, { snippet: res.data.snippet }),
          error: () => patchState(store, { snippet: null }),
        });
      }
    },
    updateFormState(partial: Partial<WidgetInstance>): void {
      const current = store.formState();
      patchState(store, {
        formState: current ? { ...current, ...partial } : partial,
      });
    },
    createInstance(payload: CreateWidgetInstancePayload): void {
      patchState(store, { saving: true, error: null });
      api.create(payload).subscribe({
        next: (res) => {
          patchState(store, {
            instances: [...store.instances(), res.data],
            saving: false,
          });
        },
        error: (err: unknown) =>
          patchState(store, {
            saving: false,
            error: (err as ApiError).message,
          }),
      });
    },
    updateInstance(id: string, payload: UpdateWidgetInstancePayload): void {
      patchState(store, { saving: true, error: null });
      api.update(id, payload).subscribe({
        next: (res) => {
          const updated = store.instances().map((i) => (i.id === id ? res.data : i));
          patchState(store, {
            instances: updated,
            selectedId: id,
            formState: { ...res.data },
            saving: false,
          });
          api.getSnippet(id).subscribe({
            next: (snippetRes) => patchState(store, { snippet: snippetRes.data.snippet }),
            error: () => patchState(store, { snippet: null }),
          });
        },
        error: (err: unknown) =>
          patchState(store, {
            saving: false,
            error: (err as ApiError).message,
          }),
      });
    },
    deleteInstance(id: string): void {
      patchState(store, { saving: true, error: null });
      api.delete(id).subscribe({
        next: () => {
          const filtered = store.instances().filter((i) => i.id !== id);
          const nextSelected = store.selectedId() === id ? null : store.selectedId();
          patchState(store, {
            instances: filtered,
            selectedId: nextSelected,
            formState: nextSelected
              ? { ...(filtered.find((i) => i.id === nextSelected) ?? {}) }
              : null,
            snippet: null,
            saving: false,
          });
        },
        error: (err: unknown) =>
          patchState(store, {
            saving: false,
            error: (err as ApiError).message,
          }),
      });
    },
    clearError(): void {
      patchState(store, { error: null });
    },
  })),
  withHooks({
    onInit(store) {
      store.loadList();
    },
  }),
);
