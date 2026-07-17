import { describe, it, expect } from 'vitest';
import {
  scanPlaceholders,
  validatePrompt,
  renderPreview,
  diffLines,
  CATALOG_VARIABLES,
} from './prompt-lang';
// This fixture must match specs/018-prompt-management/contracts/prompt-validation-fixture.json (canonical copy)
import fixture from './prompt-validation-fixture.json';

const catalogNames = CATALOG_VARIABLES.map((v) => v.name);

interface FixtureCase {
  content?: string;
  contentRepeat?: { unit: string; count: number };
  expected: { code: string }[];
}

function expandFixtureCase(tc: FixtureCase): string {
  if (tc.contentRepeat) {
    return tc.contentRepeat.unit.repeat(tc.contentRepeat.count);
  }
  return tc.content!;
}

describe('prompt-lang', () => {
  describe('validatePrompt', () => {
    for (const tc of fixture as FixtureCase[]) {
      const content = expandFixtureCase(tc);
      it(`handles: ${JSON.stringify(content.slice(0, 40))}`, () => {
        const issues = validatePrompt(content, catalogNames);
        const codes = issues.map((i) => i.code);
        expect(codes).toEqual(tc.expected.map((e) => e.code));
      });
    }
  });

  describe('renderPreview', () => {
    it('substitutes known variables with samples', () => {
      const result = renderPreview('Hello {{agent_name}} from {{tenant_name}}', {
        agent_name: 'Aria',
        tenant_name: 'Acme Support',
      });
      expect(result.text).toBe('Hello Aria from Acme Support');
      expect(result.errorSpans).toEqual([]);
    });

    it('is deterministic', () => {
      const samples = { agent_name: 'Aria', tenant_name: 'Acme' };
      const a = renderPreview('Hi {{agent_name}} from {{tenant_name}}', samples);
      const b = renderPreview('Hi {{agent_name}} from {{tenant_name}}', samples);
      expect(a.text).toBe(b.text);
      expect(a.errorSpans).toEqual(b.errorSpans);
    });

    it('does not re-scan substituted values (injection safety)', () => {
      const result = renderPreview('Hello {{agent_name}}', { agent_name: '{{customer_name}}' });
      expect(result.text).toBe('Hello {{customer_name}}');
      expect(result.errorSpans).toEqual([]);
    });

    it('passes through unknown placeholders as-is', () => {
      const content = 'Hours: {{business_hours}}';
      const result = renderPreview(content, {});
      expect(result.text).toBe(content);
      expect(result.errorSpans).toHaveLength(1);
      expect(result.errorSpans[0].reason).toBe('unknown');
    });

    it('passes through malformed placeholders as-is', () => {
      const content = '{{agent_name';
      const result = renderPreview(content, {});
      expect(result.text).toBe(content);
      expect(result.errorSpans).toHaveLength(1);
      expect(result.errorSpans[0].reason).toBe('malformed');
    });

    it('records error spans for malformed placeholders', () => {
      const result = renderPreview('{{}}', {});
      expect(result.text).toBe('{{}}');
      expect(result.errorSpans).toHaveLength(1);
      expect(result.errorSpans[0].reason).toBe('malformed');
    });

    it('records error spans for unknown placeholders', () => {
      const result = renderPreview('{{business_hours}}', {});
      expect(result.text).toBe('{{business_hours}}');
      expect(result.errorSpans).toHaveLength(1);
      expect(result.errorSpans[0].reason).toBe('unknown');
    });
  });

  describe('diffLines', () => {
    it('identical strings produce all same', () => {
      const lines = diffLines('hello\nworld', 'hello\nworld');
      expect(lines).toEqual([
        { kind: 'same', text: 'hello' },
        { kind: 'same', text: 'world' },
      ]);
    });

    it('one added line', () => {
      const lines = diffLines('hello', 'hello\nworld');
      expect(lines).toEqual([
        { kind: 'same', text: 'hello' },
        { kind: 'added', text: 'world' },
      ]);
    });

    it('one removed line', () => {
      const lines = diffLines('hello\nworld', 'hello');
      expect(lines).toEqual([
        { kind: 'same', text: 'hello' },
        { kind: 'removed', text: 'world' },
      ]);
    });

    it('completely different', () => {
      const lines = diffLines('aaa', 'bbb');
      expect(lines).toEqual([
        { kind: 'removed', text: 'aaa' },
        { kind: 'added', text: 'bbb' },
      ]);
    });

    it('multi-line with common prefix and suffix', () => {
      const lines = diffLines('a\nb\nc\nd\ne', 'a\nx\ny\nd\ne');
      expect(lines).toEqual([
        { kind: 'same', text: 'a' },
        { kind: 'removed', text: 'b' },
        { kind: 'removed', text: 'c' },
        { kind: 'added', text: 'x' },
        { kind: 'added', text: 'y' },
        { kind: 'same', text: 'd' },
        { kind: 'same', text: 'e' },
      ]);
    });

    it('empty strings', () => {
      expect(diffLines('', '')).toEqual([]);
      expect(diffLines('a', '')).toEqual([{ kind: 'removed', text: 'a' }]);
      expect(diffLines('', 'b')).toEqual([{ kind: 'added', text: 'b' }]);
    });
  });

  describe('scanPlaceholders', () => {
    it('finds valid placeholders', () => {
      const spans = scanPlaceholders('Hello {{agent_name}} from {{tenant_name}}');
      expect(spans).toHaveLength(2);
      expect(spans[0]).toEqual({ start: 6, end: 20, name: 'agent_name', valid: true });
      expect(spans[1]).toEqual({ start: 26, end: 41, name: 'tenant_name', valid: true });
    });

    it('marks invalid placeholders', () => {
      const spans = scanPlaceholders('{{Agent_Name}}');
      expect(spans).toHaveLength(1);
      expect(spans[0].valid).toBe(false);
      expect(spans[0].name).toBe('Agent_Name');
    });

    it('marks unclosed placeholder', () => {
      const spans = scanPlaceholders('{{agent_name');
      expect(spans).toHaveLength(1);
      expect(spans[0].valid).toBe(false);
      expect(spans[0].name).toBe('agent_name');
    });

    it('handles empty content', () => {
      expect(scanPlaceholders('')).toEqual([]);
    });

    it('ignores single braces', () => {
      expect(scanPlaceholders('{agent_name}')).toEqual([]);
    });
  });
});
