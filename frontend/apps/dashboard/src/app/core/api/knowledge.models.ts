export type KnowledgeItemType = 'article' | 'faq' | 'document';
export type KnowledgeItemStatus = 'draft' | 'published' | 'archived';

export interface KnowledgeItemSummary {
  id: string;
  itemType: KnowledgeItemType;
  title: string;
  status: KnowledgeItemStatus;
  categoryId: string | null;
  categoryName: string | null;
  createdByDisplay: string;
  createdAt: string;
  updatedAt: string;
  tags: string[];
}

export interface DocumentMeta {
  originalFilename: string;
  contentType: string;
  sizeBytes: number;
  createdAt: string;
}

export interface KnowledgeItemDetail extends KnowledgeItemSummary {
  body: string | null;
  source: 'authored' | 'uploaded';
  createdByUserId: string | null;
  document: DocumentMeta | null;
}

export interface ItemListResponse {
  items: KnowledgeItemSummary[];
  hasMore: boolean;
  nextCursor: string | null;
}

export interface CreateItemPayload {
  title: string;
  body?: string | null;
  itemType: KnowledgeItemType;
  categoryId?: string | null;
  tags?: string[];
}

export interface UpdateItemPayload {
  title?: string;
  body?: string | null;
  itemType?: KnowledgeItemType;
  categoryId?: string | null;
  tags?: string[];
}

export interface SetStatusPayload {
  status: KnowledgeItemStatus;
}

export interface SetStatusResponse {
  id: string;
  status: KnowledgeItemStatus;
  changed: boolean;
  updatedAt: string;
}

export interface KnowledgeCategory {
  id: string;
  name: string;
  itemCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateCategoryPayload {
  name: string;
}

export interface RenameCategoryPayload {
  name: string;
}

export interface ItemFilters {
  type?: KnowledgeItemType;
  status?: KnowledgeItemStatus;
  categoryId?: string;
  tag?: string;
  q?: string;
}
