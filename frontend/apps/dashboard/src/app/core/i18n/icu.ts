// ICU MessageFormat for the dashboard's catalogs (MOH-155, ADR-016), without `eval`.
//
// The usual library (`@messageformat/core`, behind `@jsverse/transloco-messageformat`) builds a
// function from each message with the `Function` constructor. The dashboard's policy forbids that:
// `script-src 'self'` has no `unsafe-eval` and Trusted Types refuses a string turned into code
// (MOH-170). This file reads a message into a small tree and walks it with `Intl.PluralRules`,
// `Intl.NumberFormat` and `Intl.DateTimeFormat`, so it runs under the policy and costs about 2 kB.
//
// Supported: `{name}`, `{n, plural, =0 {..} one {..} other {..}}` (with `offset:`), `selectordinal`,
// `select`, `#`, `{n, number}` (`integer`, `percent`), `{d, date}` and `{d, time}` (`short`,
// `medium`, `long`, `full`), and `'` quoting (`''` is an apostrophe, `'{'` is a brace). Anything
// else is an error, which the transpiler raises in development and tests.

export type IcuNode =
  | string
  | { readonly t: 'arg'; readonly name: string }
  | {
      readonly t: 'fmt';
      readonly name: string;
      readonly kind: 'number' | 'date' | 'time';
      readonly style: string | null;
    }
  | { readonly t: 'hash' }
  | {
      readonly t: 'select' | 'plural' | 'ordinal';
      readonly name: string;
      readonly offset: number;
      readonly cases: ReadonlyMap<string, readonly IcuNode[]>;
    };

export type IcuParams = Readonly<Record<string, unknown>>;

const NAME = /[A-Za-z0-9_.-]/;

class Parser {
  private pos = 0;

  constructor(private readonly src: string) {}

  parse(): IcuNode[] {
    const nodes = this.nodes(false);
    if (this.pos < this.src.length) this.fail('unexpected "}"');
    return nodes;
  }

  private fail(reason: string): never {
    throw new Error(`${reason} at position ${this.pos} in "${this.src}"`);
  }

  private nodes(inPlural: boolean): IcuNode[] {
    const out: IcuNode[] = [];
    let text = '';
    const flush = () => {
      if (text) out.push(text);
      text = '';
    };
    while (this.pos < this.src.length) {
      const ch = this.src[this.pos];
      if (ch === '}') break;
      if (ch === '{') {
        flush();
        out.push(this.argument());
      } else if (ch === '#' && inPlural) {
        flush();
        out.push({ t: 'hash' });
        this.pos++;
      } else if (ch === "'") {
        text += this.quote(inPlural);
      } else {
        text += ch;
        this.pos++;
      }
    }
    flush();
    return out;
  }

  /** `''` is an apostrophe; `'` before `{`, `}` (or `#` in a plural) quotes up to the next `'`. */
  private quote(inPlural: boolean): string {
    const next = this.src[this.pos + 1];
    if (next === "'") {
      this.pos += 2;
      return "'";
    }
    if (next === '{' || next === '}' || (inPlural && next === '#')) {
      let out = '';
      this.pos++;
      while (this.pos < this.src.length) {
        const ch = this.src[this.pos];
        if (ch === "'") {
          if (this.src[this.pos + 1] === "'") {
            out += "'";
            this.pos += 2;
            continue;
          }
          this.pos++;
          return out;
        }
        out += ch;
        this.pos++;
      }
      return out;
    }
    this.pos++;
    return "'";
  }

  private space(): void {
    while (this.pos < this.src.length && /\s/.test(this.src[this.pos])) this.pos++;
  }

  private word(): string {
    this.space();
    const start = this.pos;
    while (this.pos < this.src.length && NAME.test(this.src[this.pos])) this.pos++;
    if (start === this.pos) this.fail('a name was expected');
    return this.src.slice(start, this.pos);
  }

  private expect(ch: string): void {
    this.space();
    if (this.src[this.pos] !== ch) this.fail(`"${ch}" was expected`);
    this.pos++;
  }

  private argument(): IcuNode {
    this.expect('{');
    const name = this.word();
    this.space();
    if (this.src[this.pos] === '}') {
      this.pos++;
      return { t: 'arg', name };
    }
    this.expect(',');
    const kind = this.word();
    this.space();
    if (kind === 'number' || kind === 'date' || kind === 'time') {
      let style: string | null = null;
      if (this.src[this.pos] === ',') {
        this.pos++;
        style = this.word();
      }
      this.expect('}');
      return { t: 'fmt', name, kind, style };
    }
    if (kind !== 'plural' && kind !== 'selectordinal' && kind !== 'select') {
      this.fail(`unsupported format "${kind}"`);
    }
    this.expect(',');
    let offset = 0;
    const cases = new Map<string, readonly IcuNode[]>();
    for (;;) {
      this.space();
      if (this.src[this.pos] === '}') break;
      const key = this.selector();
      if (key.startsWith('offset:')) {
        offset = Number(key.slice(7));
        if (!Number.isFinite(offset)) this.fail('a number was expected after "offset:"');
        continue;
      }
      this.expect('{');
      cases.set(key, this.nodes(kind !== 'select'));
      this.expect('}');
    }
    this.expect('}');
    if (!cases.has('other')) this.fail(`"${name}" needs an "other" case`);
    return { t: kind === 'selectordinal' ? 'ordinal' : kind, name, offset, cases };
  }

  private selector(): string {
    this.space();
    const start = this.pos;
    while (this.pos < this.src.length && /[^\s{}]/.test(this.src[this.pos])) this.pos++;
    if (start === this.pos) this.fail('a case was expected');
    return this.src.slice(start, this.pos);
  }
}

/** Reads a message into a tree. Throws on a syntax error. */
export function parseMessage(source: string): IcuNode[] {
  return new Parser(source).parse();
}

function numeric(value: unknown, name: string): number {
  const n = typeof value === 'number' ? value : Number(value);
  if (value === null || value === '' || !Number.isFinite(n)) {
    throw new Error(`"${name}" must be a number`);
  }
  return n;
}

function dateOf(value: unknown, name: string): Date {
  const date = value instanceof Date ? value : new Date(value as string | number);
  if (Number.isNaN(date.getTime())) throw new Error(`"${name}" must be a date`);
  return date;
}

const DATE_STYLES = new Set(['short', 'medium', 'long', 'full']);

/** Writes a parsed message for a locale. Throws when a value it needs is missing or wrong. */
export function formatMessage(
  nodes: readonly IcuNode[],
  locale: string,
  params: IcuParams,
): string {
  const cardinal = new Intl.PluralRules(locale);
  const ordinal = new Intl.PluralRules(locale, { type: 'ordinal' });
  const numberFormat = new Intl.NumberFormat(locale);

  const read = (name: string): unknown => {
    if (!(name in params)) throw new Error(`missing value for "${name}"`);
    return params[name];
  };

  const walk = (list: readonly IcuNode[], hash: string): string => {
    let out = '';
    for (const node of list) {
      if (typeof node === 'string') {
        out += node;
        continue;
      }
      switch (node.t) {
        case 'arg':
          out += String(read(node.name));
          break;
        case 'hash':
          out += hash;
          break;
        case 'fmt': {
          const value = read(node.name);
          if (node.kind === 'number') {
            const options: Intl.NumberFormatOptions | undefined =
              node.style === 'percent'
                ? { style: 'percent' }
                : node.style === 'integer'
                  ? { maximumFractionDigits: 0 }
                  : undefined;
            out += new Intl.NumberFormat(locale, options).format(numeric(value, node.name));
          } else {
            const style = node.style && DATE_STYLES.has(node.style) ? node.style : 'medium';
            const options = (
              node.kind === 'date' ? { dateStyle: style } : { timeStyle: style }
            ) as Intl.DateTimeFormatOptions;
            out += new Intl.DateTimeFormat(locale, options).format(dateOf(value, node.name));
          }
          break;
        }
        case 'select': {
          const chosen = node.cases.get(String(read(node.name))) ?? node.cases.get('other');
          out += walk(chosen ?? [], hash);
          break;
        }
        default: {
          const n = numeric(read(node.name), node.name);
          const shifted = n - node.offset;
          const rules = node.t === 'plural' ? cardinal : ordinal;
          const chosen = node.cases.get(`=${n}`) ?? node.cases.get(rules.select(shifted));
          out += walk(chosen ?? node.cases.get('other') ?? [], numberFormat.format(shifted));
        }
      }
    }
    return out;
  };

  return walk(nodes, '#');
}
