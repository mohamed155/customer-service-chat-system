export interface PlaceholderSpan {
  start: number;
  end: number;
  name: string;
  valid: boolean;
}

export interface ValidationIssue {
  field: string;
  code: 'required' | 'too_long' | 'malformed_placeholder' | 'unknown_variable';
  message: string;
}

export interface PreviewResult {
  text: string;
  errorSpans: { start: number; end: number; reason: string }[];
}

export const CATALOG_VARIABLES: { name: string; description: string; sample: string }[] = [
  { name: 'agent_name', description: "The AI agent's customer-facing name", sample: 'Aria' },
  { name: 'tenant_name', description: "The tenant's business name", sample: 'Acme Support' },
  { name: 'customer_name', description: "The customer's display name", sample: 'Jamie Lee' },
  { name: 'channel', description: "The conversation's channel", sample: 'web_chat' },
];

export const MAX_CONTENT_LENGTH = 8000;

function isValidVariableName(name: string): boolean {
  if (name.length === 0) return false;
  const first = name.charCodeAt(0);
  if (!(first >= 97 && first <= 122)) return false;
  for (let i = 1; i < name.length; i++) {
    const c = name.charCodeAt(i);
    if (!(c >= 97 && c <= 122) && !(c >= 48 && c <= 57) && c !== 95) {
      return false;
    }
  }
  return true;
}

export function scanPlaceholders(content: string): PlaceholderSpan[] {
  const spans: PlaceholderSpan[] = [];
  const len = content.length;
  let i = 0;
  let inPlaceholder = false;
  let placeholderStart = 0;
  let name = '';

  while (i < len) {
    if (!inPlaceholder) {
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
    } else {
      if (content[i] === '}' && i + 1 < len && content[i + 1] === '}') {
        const end = i + 2;
        const valid = name.length > 0 && isValidVariableName(name);
        spans.push({ start: placeholderStart, end, name, valid });
        inPlaceholder = false;
        i += 2;
        continue;
      }
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        spans.push({ start: placeholderStart, end: i, name, valid: false });
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
      name += content[i];
    }
    i += 1;
  }

  if (inPlaceholder) {
    spans.push({ start: placeholderStart, end: content.length, name, valid: false });
  }

  return spans;
}

export function validatePrompt(content: string, catalogNames: string[]): ValidationIssue[] {
  const issues: ValidationIssue[] = [];

  const trimmed = content.trim();
  if ([...trimmed].length === 0) {
    issues.push({ field: 'content', code: 'required', message: 'prompt content is required' });
  }

  if ([...content].length > MAX_CONTENT_LENGTH) {
    issues.push({
      field: 'content',
      code: 'too_long',
      message: `prompt content must not exceed ${MAX_CONTENT_LENGTH} characters`,
    });
  }

  const len = content.length;
  let i = 0;
  let inPlaceholder = false;
  let placeholderStart = 0;
  let name = '';

  while (i < len) {
    if (!inPlaceholder) {
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
      if (content[i] === '}' && i + 1 < len && content[i + 1] === '}') {
        issues.push({
          field: 'content',
          code: 'malformed_placeholder',
          message: `stray closing braces at offset ${i}`,
        });
        i += 2;
        continue;
      }
    } else {
      if (content[i] === '}' && i + 1 < len && content[i + 1] === '}') {
        if (name.length === 0) {
          issues.push({
            field: 'content',
            code: 'malformed_placeholder',
            message: `empty placeholder at offset ${placeholderStart}`,
          });
        } else if (!isValidVariableName(name)) {
          issues.push({
            field: 'content',
            code: 'malformed_placeholder',
            message: `invalid placeholder '${name}' at offset ${placeholderStart}`,
          });
        } else if (!catalogNames.includes(name)) {
          issues.push({
            field: 'content',
            code: 'unknown_variable',
            message: `unknown variable '${name}' at offset ${placeholderStart}`,
          });
        }
        inPlaceholder = false;
        i += 2;
        continue;
      }
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        issues.push({
          field: 'content',
          code: 'malformed_placeholder',
          message: `unclosed placeholder at offset ${placeholderStart}`,
        });
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
      name += content[i];
    }
    i += 1;
  }

  if (inPlaceholder) {
    issues.push({
      field: 'content',
      code: 'malformed_placeholder',
      message: `unclosed placeholder at offset ${placeholderStart}`,
    });
  }

  return issues;
}

export function renderPreview(content: string, samples: Record<string, string>): PreviewResult {
  const errorSpans: { start: number; end: number; reason: string }[] = [];
  let output = '';
  const len = content.length;
  let i = 0;
  let inPlaceholder = false;
  let placeholderStart = 0;
  let name = '';
  let lastEnd = 0;

  while (i < len) {
    if (!inPlaceholder) {
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        output += content.slice(lastEnd, i);
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
    } else {
      if (content[i] === '}' && i + 1 < len && content[i + 1] === '}') {
        const endOffset = i + 2;
        const valid = name.length > 0 && isValidVariableName(name);

        if (valid && samples[name] !== undefined) {
          output += samples[name];
        } else {
          output += content.slice(placeholderStart, endOffset);
          if (!valid || !(name in samples)) {
            errorSpans.push({
              start: placeholderStart,
              end: endOffset,
              reason: !valid ? 'malformed' : 'unknown',
            });
          }
        }

        inPlaceholder = false;
        lastEnd = endOffset;
        i += 2;
        continue;
      }
      if (content[i] === '{' && i + 1 < len && content[i + 1] === '{') {
        output += content.slice(placeholderStart, i);
        errorSpans.push({
          start: placeholderStart,
          end: i,
          reason: 'malformed',
        });
        inPlaceholder = true;
        placeholderStart = i;
        name = '';
        i += 2;
        continue;
      }
      name += content[i];
    }
    i += 1;
  }

  if (inPlaceholder) {
    output += content.slice(placeholderStart);
    errorSpans.push({
      start: placeholderStart,
      end: content.length,
      reason: 'malformed',
    });
  } else {
    output += content.slice(lastEnd);
  }

  return { text: output, errorSpans };
}

export interface DiffLine {
  kind: 'same' | 'added' | 'removed';
  text: string;
}

export function diffLines(previous: string, current: string): DiffLine[] {
  if (previous === current && previous === '') return [];

  const prevLines = previous.length === 0 ? [] : previous.split('\n');
  const currLines = current.length === 0 ? [] : current.split('\n');

  let prefixLen = 0;
  const maxPrefix = Math.min(prevLines.length, currLines.length);
  while (prefixLen < maxPrefix && prevLines[prefixLen] === currLines[prefixLen]) {
    prefixLen++;
  }

  let suffixLen = 0;
  const maxSuffix = Math.min(prevLines.length - prefixLen, currLines.length - prefixLen);
  while (
    suffixLen < maxSuffix &&
    prevLines[prevLines.length - 1 - suffixLen] === currLines[currLines.length - 1 - suffixLen]
  ) {
    suffixLen++;
  }

  const result: DiffLine[] = [];

  for (let i = 0; i < prefixLen; i++) {
    result.push({ kind: 'same', text: prevLines[i] });
  }

  const prevMidEnd = prevLines.length - suffixLen;
  const currMidEnd = currLines.length - suffixLen;

  for (let i = prefixLen; i < prevMidEnd; i++) {
    result.push({ kind: 'removed', text: prevLines[i] });
  }
  for (let i = prefixLen; i < currMidEnd; i++) {
    result.push({ kind: 'added', text: currLines[i] });
  }

  for (let i = prevLines.length - suffixLen; i < prevLines.length; i++) {
    result.push({ kind: 'same', text: prevLines[i] });
  }

  return result;
}
