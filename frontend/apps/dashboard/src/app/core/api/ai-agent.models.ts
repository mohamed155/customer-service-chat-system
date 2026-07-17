export interface AgentConfigResponse {
  configured: boolean;
  agent: AgentDetail;
}

export interface AgentDetail {
  id: string | null;
  name: string;
  isDefault: boolean;
  avatar: AvatarInfo;
  tone: string;
  activePrompt: ActivePromptInfo | null;
  businessRules: string[];
  escalationRules: EscalationRuleDetail[];
  enabledChannels: string[];
  providerSelection: ProviderSelectionInfo;
  version: number | null;
  updatedAt: string | null;
}

export interface ActivePromptInfo {
  version: number;
  updatedAt: string | null;
  updatedBy: string | null;
  excerpt: string;
}

export interface AvatarInfo {
  kind: 'preset' | 'upload';
  preset: string | null;
  uploadUrl: string | null;
}

export interface EscalationRuleDetail {
  id: string;
  name: string;
  trigger: 'human_request' | 'topic_keywords';
  keywords: string[];
  requiredSkillIds: string[];
  brokenSkillRefs: string[];
}

export interface ProviderSelectionInfo {
  provider: string | null;
  model: string | null;
  stale: boolean;
}

export interface AgentConfigPayload {
  name: string;
  avatar: { kind: 'preset'; preset: string } | { kind: 'upload' };
  tone: string;
  businessRules: string[];
  escalationRules: EscalationRulePayload[];
  enabledChannels: string[];
  providerSelection?: { provider: string; model: string } | null;
  version?: number | null;
}

export interface EscalationRulePayload {
  id?: string;
  name: string;
  trigger: 'human_request' | 'topic_keywords';
  keywords: string[];
  requiredSkillIds: string[];
}

export interface AgentOptionsResponse {
  tones: string[];
  channels: string[];
  avatarPresets: string[];
  providers: ProviderOption[];
  aiLayerDefault: AiLayerDefaultInfo;
  promptMaxLength: number;
  limits: LimitsInfo;
}

export interface ProviderOption {
  provider: string;
  credentialAvailable: boolean;
  models: string[];
}

export interface AiLayerDefaultInfo {
  provider: string | null;
  model: string | null;
}

export interface LimitsInfo {
  businessRulesMax: number;
  escalationRulesMax: number;
}

export interface AvatarUpdateResponse {
  avatar: AvatarInfo;
  version: number;
}

export interface AiHandlingPayload {
  mode: 'platform_ai' | 'human';
}

export interface PromptVariable {
  name: string;
  description: string;
  sample: string;
}

export interface PromptLimits {
  maxContentLength: number;
  maxChangeNoteLength: number;
}

export interface PromptBootstrapResponse {
  prompt: {
    exists: boolean;
    activeVersion: number;
    content: string;
    updatedAt: string | null;
    updatedBy: string | null;
  };
  variables: PromptVariable[];
  limits: PromptLimits;
}

export interface PromptSavePayload {
  content: string;
  changeNote: string | null;
  baseVersion: number;
}

export interface PromptSaveResponse {
  version: number;
  created: boolean;
  updatedAt: string;
  updatedBy: string;
  restoredFrom?: number;
}

export interface PromptVersionListItem {
  versionNumber: number;
  contentPreview: string;
  changeNote: string | null;
  restoredFrom: number | null;
  createdAt: string;
  createdBy: string;
  isActive: boolean;
}

export interface PromptVersionListResponse {
  items: PromptVersionListItem[];
  hasMore: boolean;
}

export interface PromptVersionDetail {
  versionNumber: number;
  content: string;
  changeNote: string | null;
  restoredFrom: number | null;
  createdAt: string;
  createdBy: string;
  isActive: boolean;
}

export interface RestorePayload {
  baseVersion: number;
}
