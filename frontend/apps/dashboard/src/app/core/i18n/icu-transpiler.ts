import { Injectable, isDevMode } from '@angular/core';
import { DefaultTranspiler, type TranspileParams } from '@jsverse/transloco';
import { type IcuNode, type IcuParams, formatMessage, parseMessage } from './icu';
import { DEFAULT_LOCALE } from './locale';

/**
 * Transloco's transpiler with ICU MessageFormat on top of its own `{{ name }}` interpolation
 * (`icu.ts` says why this is not `@jsverse/transloco-messageformat`). A message with no `{`, `}`
 * or `'` is returned as it is, which is nearly all of them.
 */
@Injectable()
export class IcuTranspiler extends DefaultTranspiler {
  private locale = DEFAULT_LOCALE;
  private readonly parsed = new Map<string, readonly IcuNode[]>();

  override transpile(options: TranspileParams): unknown {
    const value = super.transpile(options);
    if (typeof value !== 'string' || !/[{}']/.test(value)) return value;
    try {
      let nodes = this.parsed.get(value);
      if (!nodes) {
        nodes = parseMessage(value);
        this.parsed.set(value, nodes);
      }
      return formatMessage(nodes, this.locale, (options.params ?? {}) as IcuParams);
    } catch (error) {
      // A catalog mistake. In development and tests it must be loud; in production a person
      // sees the text as written rather than nothing.
      if (isDevMode()) {
        throw new Error(`Bad message "${options.key ?? ''}": ${(error as Error).message}`, {
          cause: error,
        });
      }
      return value;
    }
  }

  onLangChanged(lang: string): void {
    this.locale = lang;
  }
}
