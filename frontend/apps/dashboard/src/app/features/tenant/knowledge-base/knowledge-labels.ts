import { marker } from '../../../core/i18n/marker';
import { ownValue } from '../../../core/i18n/own-value';

/** Catalog keys of the names of an item's status, type and index status. A value this app does
 * not know is shown as the API sent it. */
const STATUS_KEYS: Readonly<Record<string, string>> = {
  draft: marker('knowledge-base.status.draft'),
  published: marker('knowledge-base.status.published'),
  archived: marker('knowledge-base.status.archived'),
};

const TYPE_KEYS: Readonly<Record<string, string>> = {
  article: marker('knowledge-base.type.article'),
  faq: marker('knowledge-base.type.faq'),
  document: marker('knowledge-base.type.document'),
};

const INDEX_KEYS: Readonly<Record<string, string>> = {
  not_indexed: marker('knowledge-base.index.notIndexed'),
  pending: marker('knowledge-base.index.pending'),
  indexing: marker('knowledge-base.index.indexing'),
  indexed: marker('knowledge-base.index.indexed'),
  failed: marker('knowledge-base.index.failed'),
  not_indexable: marker('knowledge-base.index.notIndexable'),
};

/**
 * What a failed or unreadable item's index status says to the person. The API's own
 * `failureReason` is an English sentence that may carry a provider's error, so it is not shown.
 */
const INDEX_HINT_KEYS: Readonly<Record<string, string>> = {
  failed: marker('knowledge-base.index.failedHint'),
  not_indexable: marker('knowledge-base.index.notIndexableHint'),
};

export const statusKey = (status: string): string | null => ownValue(STATUS_KEYS, status) ?? null;
export const typeKey = (type: string): string | null => ownValue(TYPE_KEYS, type) ?? null;
export const indexKey = (status: string): string | null => ownValue(INDEX_KEYS, status) ?? null;
export const indexHintKey = (status: string): string | null =>
  ownValue(INDEX_HINT_KEYS, status) ?? null;

export const statusTone = (status: string): 'green' | 'amber' | 'red' =>
  status === 'published' ? 'green' : status === 'draft' ? 'amber' : 'red';
