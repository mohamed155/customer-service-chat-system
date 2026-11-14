import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, pipe, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../../core/api/api.models';
import {
  PromptBootstrapResponse,
  PromptSavePayload,
  PromptVersionDetail,
  PromptVersionListItem,
} from '../../../../core/api/ai-agent.models';
import { toApiError } from '../../../../core/errors/api-error';
import { agentFieldErrors } from '../ai-agent-errors';
import { PromptApiService } from './prompt-api.service';
import { withTenantScope } from '../../../../core/state/tenant-scope';

interface PromptState {
  bootstrap: PromptBootstrapResponse | null;
  editorContent: string;
  changeNote: string;
  dirty: boolean;
  loading: boolean;
  saving: boolean;
  error: ApiError | null;
  conflict: boolean;
  fieldErrors: Record<string, string[]> | null;
  noOpNotice: boolean;
  historyItems: PromptVersionListItem[];
  historyHasMore: boolean;
  historyLoading: boolean;
  selectedVersion: PromptVersionDetail | null;
}

const initialState: PromptState = {
  bootstrap: null,
  editorContent: '',
  changeNote: '',
  dirty: false,
  loading: false,
  saving: false,
  error: null,
  conflict: false,
  fieldErrors: null,
  noOpNotice: false,
  historyItems: [],
  historyHasMore: false,
  historyLoading: false,
  selectedVersion: null,
};

export const PromptStore = signalStore(
  withState(initialState),
  withMethods((store, api = inject(PromptApiService)) => ({
    load: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null, noOpNotice: false })),
        switchMap(() =>
          api.getPrompt().pipe(
            tap((res) =>
              patchState(store, {
                bootstrap: res.data,
                editorContent: res.data.prompt.content,
                changeNote: '',
                dirty: false,
                loading: false,
                error: null,
                conflict: false,
                fieldErrors: null,
                noOpNotice: false,
              }),
            ),
            catchError((err) => {
              patchState(store, { loading: false, error: toApiError(err) });
              return EMPTY;
            }),
          ),
        ),
      ),
    ),
    setContent(content: string): void {
      patchState(store, {
        editorContent: content,
        dirty: content !== (store.bootstrap()?.prompt.content ?? ''),
        noOpNotice: false,
      });
    },
    save(): void {
      const bootstrap = store.bootstrap();
      if (!bootstrap) return;
      patchState(store, {
        saving: true,
        error: null,
        conflict: false,
        fieldErrors: null,
        noOpNotice: false,
      });
      const payload: PromptSavePayload = {
        content: store.editorContent(),
        changeNote: store.changeNote() || null,
        baseVersion: bootstrap.prompt.activeVersion,
      };
      api.savePrompt(payload).subscribe({
        next: (res) => {
          if (res.data.created) {
            patchState(store, {
              bootstrap: {
                ...bootstrap,
                prompt: {
                  ...bootstrap.prompt,
                  content: store.editorContent(),
                  activeVersion: res.data.version,
                  updatedAt: res.data.updatedAt,
                  updatedBy: res.data.updatedBy,
                },
              },
              dirty: false,
              changeNote: '',
              saving: false,
              noOpNotice: false,
            });
          } else {
            patchState(store, {
              saving: false,
              noOpNotice: true,
            });
          }
        },
        error: (err: unknown) => {
          const error = toApiError(err);
          if (error.status === 409) {
            patchState(store, { saving: false, conflict: true });
          } else if (error.status === 422 && error.details) {
            patchState(store, { saving: false, fieldErrors: agentFieldErrors(error.details) });
          } else {
            patchState(store, { saving: false, error });
          }
        },
      });
    },
    dismissConflict(): void {
      patchState(store, { conflict: false });
    },
    dismissNoOpNotice(): void {
      patchState(store, { noOpNotice: false });
    },
    clearSelectedVersion(): void {
      patchState(store, { selectedVersion: null });
    },
    setChangeNote(note: string): void {
      patchState(store, { changeNote: note });
    },
    loadHistory(before?: number): void {
      patchState(store, { historyLoading: true, error: null, selectedVersion: null });
      api.listVersions(25, before).subscribe({
        next: (res) => {
          patchState(store, {
            historyItems: before ? [...store.historyItems(), ...res.data.items] : res.data.items,
            historyHasMore: res.data.hasMore,
            historyLoading: false,
          });
        },
        error: (err: unknown) => {
          patchState(store, { historyLoading: false, error: toApiError(err) });
        },
      });
    },
    selectVersion(versionNumber: number): void {
      api.getVersion(versionNumber).subscribe({
        next: (res) => {
          patchState(store, { selectedVersion: res.data });
        },
        error: (err: unknown) => {
          patchState(store, { error: toApiError(err) });
        },
      });
    },
    restore(versionNumber: number): void {
      const bootstrap = store.bootstrap();
      const selected = store.selectedVersion();
      if (!bootstrap) return;
      patchState(store, {
        saving: true,
        error: null,
        conflict: false,
        fieldErrors: null,
        noOpNotice: false,
      });
      api.restoreVersion(versionNumber, bootstrap.prompt.activeVersion).subscribe({
        next: (res) => {
          if (res.data.created) {
            patchState(store, {
              bootstrap: {
                ...bootstrap,
                prompt: {
                  ...bootstrap.prompt,
                  content: selected?.content ?? bootstrap.prompt.content,
                  activeVersion: res.data.version,
                  updatedAt: res.data.updatedAt,
                  updatedBy: res.data.updatedBy,
                },
              },
              editorContent: selected?.content ?? bootstrap.prompt.content,
              dirty: false,
              changeNote: '',
              saving: false,
              noOpNotice: false,
            });
          } else {
            patchState(store, { saving: false, noOpNotice: true });
          }
        },
        error: (err: unknown) => {
          const error = toApiError(err);
          if (error.status === 409) {
            patchState(store, { saving: false, conflict: true });
          } else if (error.status === 422 && error.details) {
            patchState(store, { saving: false, fieldErrors: agentFieldErrors(error.details) });
          } else {
            patchState(store, { saving: false, error });
          }
        },
      });
    },
  })),
  withHooks({
    onInit(store) {
      store.load();
    },
  }),
  withTenantScope({
    initialState,
    reload: (store) => store.load(),
  }),
);
